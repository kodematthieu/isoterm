// /src/error.rs

use thiserror::Error;

/// A type alias for `Result<T, anyhow::Error>` to be used throughout the application.
pub type AppResult<T> = anyhow::Result<T>;

/// Errors that are intended to be displayed directly to the user.
#[derive(Debug, Error)]
pub enum UserError {
    #[error("Could not find a compatible release asset for '{name}' on your platform ({os} {arch}).")]
    AssetNotFound {
        name: String,
        os: String,
        arch: String,
    },

    #[error("Failed to download from '{url}'. Please check your network connection.\n  Reason: {source}")]
    DownloadFailed {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("The downloaded archive is corrupted or in an unexpected format.")]
    ArchiveExtractionFailed {
        #[source]
        source: std::io::Error,
    },

    #[error("Could not find the binary '{binary_name}' inside the downloaded archive.")]
    BinaryNotFoundInArchive { binary_name: String },

    #[error("The external command '{command}' failed to execute.\n  Reason: {source}")]
    CommandFailed {
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to query the GitHub API. Please check your network connection.\n  Reason: {source}")]
    GitHubApiError {
        #[source]
        source: reqwest::Error,
    },

    #[error("Your platform ({os}) is not supported for '{name}'.")]
    UnsupportedPlatform { name: String, os: String },
}