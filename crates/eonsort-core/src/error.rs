use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    Bare(#[from] std::io::Error),

    #[error("malformed plan file at line {line}: {message}")]
    MalformedPlan { line: usize, message: String },

    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("the tag store: {0}")]
    Store(#[from] rusqlite::Error),

    #[error("could not start the copy thread pool: {0}")]
    ThreadPool(String),

    #[error("plan file has no header record")]
    MissingPlanHeader,

    #[error("plan format version {0} is not supported")]
    UnsupportedPlanVersion(u32),

    #[error("invalid folder pattern: {0}")]
    InvalidFolderPattern(String),

    #[error("source path has no file name: {0}")]
    InvalidSourcePath(PathBuf),

    #[error("could not find a free destination name for {0}")]
    DestinationExhausted(PathBuf),

    #[error("this plan has no destination folder yet")]
    NoDestination,

    #[error("cancelled")]
    Cancelled,

    #[error("{0} cannot be turned without re-encoding it")]
    RotationNotLossless(PathBuf),

    #[error("{path}: {message}")]
    Rotation { path: PathBuf, message: String },

    #[error("{0}")]
    Tagging(String),

    #[error("{0}")]
    Upright(String),

    #[error("{0}")]
    Diffuse(String),

    #[error("{0}")]
    Download(String),
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}
