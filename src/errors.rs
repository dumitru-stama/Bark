use thiserror::Error;
use crate::providers::ProviderError;

/// Application-level errors.
/// Some variants are reserved for future use as error handling is expanded.
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Operation failed: {0}")]
    Operation(String),
}

pub type AppResult<T> = Result<T, AppError>;
