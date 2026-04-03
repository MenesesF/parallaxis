//! Error types for Parallaxis.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParallaxisError {
    #[error("Vault error: {0}")]
    Vault(String),

    #[error("Extraction error: {0}")]
    Extraction(String),

    #[error("Verification error: {0}")]
    Verification(String),

    #[error("Normalization error: {0}")]
    Normalization(String),

    #[error("Entity not found: {0}")]
    EntityNotFound(String),

    #[error("Predicate not found: {0}")]
    PredicateNotFound(String),

    #[error("Domain not found: {0}")]
    DomainNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Invalid format: {0}")]
    InvalidFormat(String),
}

pub type Result<T> = std::result::Result<T, ParallaxisError>;
