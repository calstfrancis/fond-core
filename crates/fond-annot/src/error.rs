use std::path::PathBuf;

/// Failure to read or write an annotation sidecar. A concrete enum so it can cross any UI seam.
#[derive(Debug, thiserror::Error)]
pub enum AnnotError {
    #[error("failed to parse {path}: {message}")]
    PlainYaml { path: PathBuf, message: String },
}

pub type Result<T> = std::result::Result<T, AnnotError>;
