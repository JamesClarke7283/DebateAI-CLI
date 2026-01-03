//! DebateAI Core Library
//! 
//! Provides the core debate orchestration logic, format definitions,
//! AI participant management, and TTS output.

pub mod debate_format;
pub mod participant;
pub mod orchestrator;
pub mod error;
pub mod config;
pub mod tts;

pub use debate_format::{DebateFormat, DebateSection, PresidentialDebateFormat};
pub use participant::{AIParticipant, ParticipantRole};
pub use orchestrator::{DebateOrchestrator, DebateConfig, DebateMessage, DebateEvent};
pub use error::DebateError;
pub use config::{Config, VoicesConfig};
pub use tts::{DebateTts, combine_audio_segments, generate_output_filename};

