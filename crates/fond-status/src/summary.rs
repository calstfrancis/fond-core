//! A background writer for an app whose status is one summary line and nothing to resume
//! ("412 references"). The app reports the line from any thread, as often as it likes;
//! the file is read and written on one worker thread, and only when the line changes.
//!
//! Resume entries already in the file are kept, so an app can use this beside its own
//! handling of [`Status::push_resume`] as long as both run on this worker's schedule; an
//! app that writes resume entries from its own thread should write its summary there too.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::time::Duration;

use crate::{now_utc, status_dir, Status};

enum Msg {
    Set {
        summary: Option<String>,
        until: Option<String>,
    },
    Flush(Sender<()>),
}

struct Worker {
    app: String,
    tx: Sender<Msg>,
}

static WORKER: OnceLock<Option<Worker>> = OnceLock::new();

/// The worker, started on first use for `app`. `None` when there is no home directory to
/// write under. One process is one app: a later call naming a different app is ignored.
fn worker(app: &str) -> Option<&'static Sender<Msg>> {
    let worker = WORKER
        .get_or_init(|| {
            let dir = status_dir()?;
            let (tx, rx) = mpsc::channel();
            let name = app.to_string();
            std::thread::Builder::new()
                .name("fond-status".into())
                .spawn(move || run(&dir, &name, rx))
                .ok()?;
            Some(Worker {
                app: app.to_string(),
                tx,
            })
        })
        .as_ref()?;
    (worker.app == app).then_some(&worker.tx)
}

/// Report `app`'s summary line. `None` clears it. `until` is the instant the line stops
/// being true (see [`Status::summary_until`]), or `None` for a line with no expiry.
/// Returns at once; never touches the disk on the calling thread.
pub fn set(app: &str, summary: Option<String>, until: Option<String>) {
    if let Some(tx) = worker(app) {
        let _ = tx.send(Msg::Set { summary, until });
    }
}

/// Wait, briefly, for everything reported so far to reach the disk. For shutdown.
pub fn flush() {
    if let Some(Some(worker)) = WORKER.get() {
        let (done, wait) = mpsc::channel();
        if worker.tx.send(Msg::Flush(done)).is_ok() {
            let _ = wait.recv_timeout(Duration::from_millis(500));
        }
    }
}

fn run(dir: &Path, app: &str, rx: Receiver<Msg>) {
    // What this process last put on disk. Unknown until the first report, so the first
    // report is always written, which also refreshes `written`.
    let mut last: Option<(Option<String>, Option<String>)> = None;
    for msg in rx {
        let (summary, until) = match msg {
            Msg::Set { summary, until } => (summary, until),
            Msg::Flush(done) => {
                let _ = done.send(());
                continue;
            }
        };
        let summary = summary
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let until = until.filter(|_| summary.is_some());
        let next = (summary, until);
        if last.as_ref() == Some(&next) {
            continue;
        }
        // Start from the file as it is, to keep anything else in it. One that cannot be
        // read is replaced rather than kept.
        let mut status = Status::read(dir, app)
            .ok()
            .flatten()
            .unwrap_or_else(|| Status::new(app, ""));
        status.app = app.to_string();
        status.summary = next.0.clone();
        status.summary_until = next.1.clone();
        status.written = now_utc();
        match status.write(dir) {
            Ok(()) => last = Some(next),
            Err(e) => eprintln!("{app}: could not write the Porog status file: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResumeEntry;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fond-status-summary-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn set(summary: Option<&str>, until: Option<&str>) -> Msg {
        Msg::Set {
            summary: summary.map(str::to_string),
            until: until.map(str::to_string),
        }
    }

    #[test]
    fn the_last_line_reported_is_the_one_on_disk() {
        let dir = scratch("last");
        let (tx, rx) = mpsc::channel();
        tx.send(set(Some("411 references"), None)).unwrap();
        tx.send(set(Some(" 412 references "), None)).unwrap();
        drop(tx);
        run(&dir, "kartoteka", rx);

        let status = Status::read(&dir, "kartoteka").unwrap().unwrap();
        assert_eq!(status.summary.as_deref(), Some("412 references"));
        assert_eq!(status.summary_until, None);
        assert!(!status.written.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clearing_the_line_drops_its_expiry_and_keeps_resume_entries() {
        let dir = scratch("clear");
        let mut before = Status::new("sputnik", "2026-11-10T12:00:00Z");
        before.push_resume(ResumeEntry {
            title: "An Example Course".to_string(),
            detail: None,
            at: None,
            args: vec!["--example".to_string()],
        });
        before.write(&dir).unwrap();

        let (tx, rx) = mpsc::channel();
        tx.send(set(
            Some("Week 6 · Example Term"),
            Some("2026-11-11T00:00:00-04:00"),
        ))
        .unwrap();
        drop(tx);
        run(&dir, "sputnik", rx);
        let status = Status::read(&dir, "sputnik").unwrap().unwrap();
        assert_eq!(status.summary.as_deref(), Some("Week 6 · Example Term"));
        assert_eq!(
            status.summary_until.as_deref(),
            Some("2026-11-11T00:00:00-04:00")
        );
        assert_eq!(status.resume.len(), 1);

        let (tx, rx) = mpsc::channel();
        tx.send(set(None, Some("2026-11-11T00:00:00-04:00")))
            .unwrap();
        drop(tx);
        run(&dir, "sputnik", rx);
        let status = Status::read(&dir, "sputnik").unwrap().unwrap();
        assert_eq!(status.summary, None);
        assert_eq!(status.summary_until, None);
        assert_eq!(status.resume.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unchanged_line_is_not_written_again() {
        let dir = scratch("unchanged");
        let (tx, rx) = mpsc::channel();
        tx.send(set(Some("12 CV elements"), None)).unwrap();
        drop(tx);
        run(&dir, "skrizhal", rx);
        let first = Status::read(&dir, "skrizhal").unwrap().unwrap();

        // Same worker, same line twice: the second is skipped, so a file removed in
        // between is not recreated.
        let (tx, rx) = mpsc::channel();
        tx.send(set(Some("12 CV elements"), None)).unwrap();
        let (done, wait) = mpsc::channel();
        tx.send(Msg::Flush(done)).unwrap();
        let handle = {
            let dir = dir.clone();
            std::thread::spawn(move || run(&dir, "skrizhal", rx))
        };
        wait.recv().unwrap();
        std::fs::remove_file(crate::status_path(&dir, "skrizhal").unwrap()).unwrap();
        tx.send(set(Some("12 CV elements"), None)).unwrap();
        drop(tx);
        handle.join().unwrap();
        assert!(Status::read(&dir, "skrizhal").unwrap().is_none());
        assert_eq!(first.summary.as_deref(), Some("12 CV elements"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
