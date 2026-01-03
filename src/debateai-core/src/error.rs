//! Error types for the debate system.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DebateError {
    #[error("Invalid participant count: expected {min}-{max}, got {actual}")]
    InvalidParticipantCount {
        min: usize,
        max: usize,
        actual: usize,
    },

    #[error("OpenAI API error: {0}")]
    OpenAIError(#[from] async_openai::error::OpenAIError),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Unknown debate format: {0}")]
    UnknownFormat(String),
}
