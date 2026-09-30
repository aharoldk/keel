//! Error type shared by every fallible operation in the crate.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// Errors returned by parsing, serializing and file helpers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The YAML text could not be parsed into the expected document.
    #[error("invalid YAML: {0}")]
    Yaml(#[source] serde_yaml_ng::Error),
    /// A document could not be turned into YAML.
    #[error("cannot serialize to YAML: {0}")]
    Serialize(#[source] serde_yaml_ng::Error),
    /// A document file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// File that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A document file could not be written.
    #[error("cannot write {path}: {source}")]
    Write {
        /// File that failed to write.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub(crate) fn read(path: &Path, source: std::io::Error) -> Self {
        Error::Read {
            path: path.to_path_buf(),
            source,
        }
    }

    pub(crate) fn write(path: &Path, source: std::io::Error) -> Self {
        Error::Write {
            path: path.to_path_buf(),
            source,
        }
    }
}

/// Convenience alias used across the crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;
