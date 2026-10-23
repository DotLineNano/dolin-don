use std::fmt;
use std::io;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, DolinError>;

#[derive(Debug)]
pub enum DolinError {
    Format(String),
    Io(io::Error),
    InvalidFormat(&'static str),
    InvalidPath(String),
    EntryNotFound(String),
    Unsupported(&'static str),
    OutputNotEmpty(PathBuf),
}

impl fmt::Display for DolinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DolinError::Format(msg) => write!(f, "format error: {}", msg),
            DolinError::Io(err) => write!(f, "io error: {}", err),
            DolinError::InvalidFormat(msg) => write!(f, "invalid format: {}", msg),
            DolinError::InvalidPath(path) => write!(f, "invalid path: {}", path),
            DolinError::EntryNotFound(path) => write!(f, "entry not found: {}", path),
            DolinError::Unsupported(msg) => write!(f, "unsupported: {}", msg),
            DolinError::OutputNotEmpty(path) => write!(
                f,
                "output directory is not empty: {} (use --force to replace, --merge to merge)",
                path.display()
            ),
        }
    }
}

impl std::error::Error for DolinError {}

impl From<serde_json::Error> for DolinError {
    fn from(err: serde_json::Error) -> Self {
        DolinError::Format(err.to_string())
    }
}

impl From<io::Error> for DolinError {
    fn from(value: io::Error) -> Self {
        DolinError::Io(value)
    }
}
