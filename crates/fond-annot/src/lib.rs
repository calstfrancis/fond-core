//! Annotation sidecar types and reading-position types shared by Kartoteka, Sputnik, Zerkalo
//! and Pereplyot. UI-agnostic: no GTK, no PDFium, no bibliography engine.

pub mod annotation;
pub mod cite;
pub mod error;
pub mod progress;
pub mod util;

pub use annotation::{Annotation, AnnotationKind, AnnotationSidecar};
pub use error::{AnnotError, Result};
pub use progress::{PageLabelOverride, Progress};
