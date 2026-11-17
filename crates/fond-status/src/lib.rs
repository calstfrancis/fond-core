//! The status file each Fond app writes so the Porog launcher can offer a way back in.
//!
//! One file per app, `~/.local/share/fond/status/<app>.json`. An app rewrites its own file
//! whenever its resume target changes; Porog only reads. UI-agnostic: no GTK, no date crate.
//!
//! Compatibility: new fields are additive and optional, a reader ignores fields it does not
//! know, and [`SCHEMA`] is bumped only for a change an older reader cannot safely ignore. A
//! reader skips a file whose `schema` is newer than its own.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The schema this crate reads and writes.
pub const SCHEMA: u32 = 1;

/// How many resume entries a status file keeps, newest first.
pub const MAX_RESUME: usize = 3;

/// The longest `context` line, in characters.
pub const MAX_CONTEXT: usize = 120;

/// `text` as one `context` line: whitespace collapsed to single spaces and cut to
/// [`MAX_CONTEXT`] characters with a trailing "…". `None` when nothing is left.
pub fn context_line(text: &str) -> Option<String> {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    if line.chars().count() <= MAX_CONTEXT {
        return Some(line);
    }
    let mut cut: String = line.chars().take(MAX_CONTEXT - 1).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    Some(cut)
}

/// Overrides [`status_dir`]. For tests and headless runs, so they never touch real files.
pub const DIR_ENV: &str = "FOND_STATUS_DIR";

#[derive(Debug, thiserror::Error)]
pub enum StatusError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("failed to parse {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("{path} uses status schema {found}; this reader knows up to {SCHEMA}")]
    NewerSchema { path: PathBuf, found: u32 },
    #[error("\"{0}\" is not a usable app name for a status file")]
    BadAppName(String),
}

pub type Result<T> = std::result::Result<T, StatusError>;

/// One way back into an app: a file, document or place it can reopen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeEntry {
    /// What the user would call it: a file name or a document title.
    pub title: String,
    /// One short phrase of context, such as a page number or a project name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// When it was last worked on, RFC 3339 with an offset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// One line from the work itself that says where you were: the heading you were under, the
    /// passage you last marked. At most [`MAX_CONTEXT`] characters; build it with [`context_line`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Handed back to the app unchanged, as `flatpak run <id> <args…>`. When the entry is a
    /// file, its absolute path is `args[0]`, so a reader can check it still exists.
    #[serde(default)]
    pub args: Vec<String>,
}

/// An app's whole status file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub schema: u32,
    /// Lowercase app name, which is also the file name: `zerkalo`, `pereplyot`, …
    pub app: String,
    /// When this file was written, RFC 3339 with an offset.
    #[serde(default)]
    pub written: String,
    /// One plain line about the app's state, shown when there is nothing to resume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The instant `summary` stops being true, RFC 3339 with an offset. For a summary that
    /// depends on the date ("Week 6 · Fall 2026"); a reader hides the summary after it.
    /// Absent for a summary that stays true until the app next writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_until: Option<String>,
    /// Newest first, at most [`MAX_RESUME`].
    #[serde(default)]
    pub resume: Vec<ResumeEntry>,
}

impl Status {
    pub fn new(app: &str, written: &str) -> Self {
        Status {
            schema: SCHEMA,
            app: app.to_string(),
            written: written.to_string(),
            summary: None,
            summary_until: None,
            resume: Vec::new(),
        }
    }

    /// Put `entry` first. An older entry with the same `args` is replaced, not repeated, and
    /// the list is cut to [`MAX_RESUME`].
    pub fn push_resume(&mut self, entry: ResumeEntry) {
        self.resume.retain(|e| e.args != entry.args);
        self.resume.insert(0, entry);
        self.resume.truncate(MAX_RESUME);
    }

    /// Drop every entry whose `args` equal `args`: the file was deleted, or the app can no
    /// longer reopen it.
    pub fn remove_resume(&mut self, args: &[String]) {
        self.resume.retain(|e| e.args != args);
    }

    /// Read `<dir>/<app>.json`. `Ok(None)` when the app has never written one.
    pub fn read(dir: &Path, app: &str) -> Result<Option<Status>> {
        let path = status_path(dir, app)?;
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(StatusError::Io { path, source }),
        };
        let mut status: Status = serde_json::from_str(&text).map_err(|e| StatusError::Parse {
            path: path.clone(),
            message: e.to_string(),
        })?;
        if status.schema > SCHEMA {
            return Err(StatusError::NewerSchema {
                path,
                found: status.schema,
            });
        }
        status.resume.truncate(MAX_RESUME);
        Ok(Some(status))
    }

    /// Write `<dir>/<app>.json`, creating `dir` if needed. Atomic: a reader sees the old
    /// file or the new one, never half of either.
    pub fn write(&self, dir: &Path) -> Result<()> {
        let path = status_path(dir, &self.app)?;
        let mut status = self.clone();
        status.schema = SCHEMA;
        status.resume.truncate(MAX_RESUME);
        let json = serde_json::to_string_pretty(&status).map_err(|e| StatusError::Parse {
            path: path.clone(),
            message: e.to_string(),
        })?;

        let io_err = |source| StatusError::Io {
            path: path.clone(),
            source,
        };
        fs::create_dir_all(dir).map_err(io_err)?;
        let tmp = dir.join(format!(".{}.json.{}.tmp", self.app, std::process::id()));
        let written = (|| {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(json.as_bytes())?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::rename(&tmp, &path)
        })();
        if written.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        written.map_err(io_err)
    }
}

/// `<dir>/<app>.json`. The app name must be plain lowercase letters, digits or `-`, so it
/// can never name a file outside `dir`.
pub fn status_path(dir: &Path, app: &str) -> Result<PathBuf> {
    let plain = !app.is_empty()
        && app
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !plain {
        return Err(StatusError::BadAppName(app.to_string()));
    }
    Ok(dir.join(format!("{app}.json")))
}

/// Where status files live: `$FOND_STATUS_DIR` if set, otherwise
/// `$HOME/.local/share/fond/status`.
///
/// Deliberately not `$XDG_DATA_HOME`: inside a flatpak that points at the app's private
/// `~/.var/app/<id>/data`, and every app has to write to the one shared directory.
pub fn status_dir() -> Option<PathBuf> {
    dir_from(std::env::var_os(DIR_ENV), std::env::var_os("HOME"))
}

fn dir_from(
    override_dir: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(dir) = override_dir.filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = home.filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".local/share/fond/status"))
}

/// The current time as RFC 3339 in UTC, to the second: `2026-11-10T20:40:12Z`. For the
/// `written` and `at` fields. Dependency-free, so no app needs a date crate to write a
/// status file; a reader converts to local time.
pub fn now_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    rfc3339_utc(secs)
}

fn rfc3339_utc(unix_secs: i64) -> String {
    let (y, m, d) = civil_from_days(unix_secs.div_euclid(86_400));
    let s = unix_secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

/// Days since 1970-01-01 to a `(year, month, day)` civil date. Howard Hinnant's algorithm,
/// public domain; the same one `fond-annot` uses.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub mod summary;

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fond-status-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn entry(title: &str, path: &str) -> ResumeEntry {
        ResumeEntry {
            title: title.to_string(),
            detail: None,
            at: Some("2026-11-09T16:40:00-04:00".to_string()),
            context: None,
            args: vec![path.to_string()],
        }
    }

    #[test]
    fn a_written_status_reads_back_the_same() {
        let dir = temp_dir("roundtrip");
        let mut status = Status::new("zerkalo", "2026-11-10T16:40:12-04:00");
        status.summary = Some("2 open projects".to_string());
        status.push_resume(entry("essay-2.typ", "/tmp/example/essay-2.typ"));
        status.write(&dir).unwrap();

        assert_eq!(Status::read(&dir, "zerkalo").unwrap(), Some(status));
        // No temp file is left behind.
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_context_line_survives_a_round_trip_and_an_old_file_has_none() {
        let dir = temp_dir("context");
        let mut status = Status::new("zerkalo", "");
        let mut e = entry("essay-2.typ", "/tmp/example/essay-2.typ");
        e.context = Some("§2 Method".to_string());
        status.push_resume(e);
        status.write(&dir).unwrap();
        let read = Status::read(&dir, "zerkalo").unwrap().unwrap();
        assert_eq!(read.resume[0].context.as_deref(), Some("§2 Method"));

        fs::write(
            dir.join("pereplyot.json"),
            r#"{"schema":1,"app":"pereplyot","resume":[{"title":"A","args":["/a.pdf"]}]}"#,
        )
        .unwrap();
        let old = Status::read(&dir, "pereplyot").unwrap().unwrap();
        assert_eq!(old.resume[0].context, None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_context_line_is_one_short_line() {
        assert_eq!(context_line("  \n "), None);
        assert_eq!(
            context_line("the second\n  argument   fails").as_deref(),
            Some("the second argument fails")
        );
        let long = context_line(&"é".repeat(300)).unwrap();
        assert_eq!(long.chars().count(), MAX_CONTEXT);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn an_app_that_never_wrote_reads_as_none() {
        let dir = temp_dir("missing");
        assert_eq!(Status::read(&dir, "skrizhal").unwrap(), None);
    }

    #[test]
    fn unknown_fields_are_ignored_and_missing_ones_default() {
        let dir = temp_dir("tolerant");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("kartoteka.json"),
            r#"{"schema":1,"app":"kartoteka","colour":"teal",
                "resume":[{"title":"A paper","pinned":true}]}"#,
        )
        .unwrap();

        let status = Status::read(&dir, "kartoteka").unwrap().unwrap();
        assert_eq!(status.written, "");
        assert_eq!(status.summary, None);
        assert_eq!(status.resume.len(), 1);
        assert!(status.resume[0].args.is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_newer_schema_and_a_corrupt_file_are_errors_not_panics() {
        let dir = temp_dir("bad");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("sputnik.json"), r#"{"schema":2,"app":"sputnik"}"#).unwrap();
        fs::write(dir.join("zerkalo.json"), "{ not json").unwrap();

        assert!(matches!(
            Status::read(&dir, "sputnik"),
            Err(StatusError::NewerSchema { found: 2, .. })
        ));
        assert!(matches!(
            Status::read(&dir, "zerkalo"),
            Err(StatusError::Parse { .. })
        ));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn push_resume_keeps_the_newest_three_without_repeats() {
        let mut status = Status::new("pereplyot", "");
        for name in ["a", "b", "c", "d"] {
            status.push_resume(entry(name, &format!("/tmp/example/{name}.pdf")));
        }
        let titles = |s: &Status| s.resume.iter().map(|e| e.title.clone()).collect::<Vec<_>>();
        assert_eq!(titles(&status), ["d", "c", "b"]);

        // Reopening one moves it to the front instead of listing it twice.
        status.push_resume(entry("b again", "/tmp/example/b.pdf"));
        assert_eq!(titles(&status), ["b again", "d", "c"]);

        status.remove_resume(&["/tmp/example/d.pdf".to_string()]);
        assert_eq!(titles(&status), ["b again", "c"]);
    }

    #[test]
    fn an_app_name_cannot_escape_the_directory() {
        let dir = Path::new("/tmp/example");
        assert!(status_path(dir, "../zerkalo").is_err());
        assert!(status_path(dir, "").is_err());
        assert!(status_path(dir, "Zerkalo").is_err());
        assert_eq!(
            status_path(dir, "zerkalo").unwrap(),
            Path::new("/tmp/example/zerkalo.json")
        );
    }

    #[test]
    fn the_directory_ignores_xdg_and_honours_the_override() {
        assert_eq!(
            dir_from(None, Some("/home/example".into())),
            Some(PathBuf::from("/home/example/.local/share/fond/status"))
        );
        assert_eq!(
            dir_from(Some("/tmp/example".into()), Some("/home/example".into())),
            Some(PathBuf::from("/tmp/example"))
        );
        assert_eq!(dir_from(None, None), None);
    }

    #[test]
    fn utc_times_are_rfc3339() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        // `date -u -d @1794343212` → 2026-11-10 20:40:12
        assert_eq!(rfc3339_utc(1_794_343_212), "2026-11-10T20:40:12Z");
        assert_eq!(now_utc().len(), 20);
    }
}
