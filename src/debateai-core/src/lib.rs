//! DebateAI Core Library
//! 
//! Provides the core debate orchestration logic, format definitions,
//! and AI participant management.

pub mod debate_format;
pub mod participant;
pub mod orchestrator;
pub mod error;

pub use debate_format::{DebateFormat, DebateSection, PresidentialDebateFormat};
pub use participant::{AIParticipant, ParticipantRole};
pub use orchestrator::{DebateOrchestrator, DebateConfig, DebateMessage, DebateEvent};
pub use error::DebateError;
