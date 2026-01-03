//! AI Participant definitions.
//!
//! Represents individual AI debaters with their configuration.

use serde::{Deserialize, Serialize};

/// Role of a participant in the debate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ParticipantRole {
    /// Arguing in favor of the topic.
    For,
    /// Arguing against the topic.
    Against,
    /// Neutral or moderating role.
    Neutral,
}

impl ParticipantRole {
    pub fn display_name(&self) -> &str {
        match self {
            ParticipantRole::For => "FOR",
            ParticipantRole::Against => "AGAINST",
            ParticipantRole::Neutral => "NEUTRAL",
        }
    }
}

/// An AI participant in the debate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIParticipant {
    /// Display name for this participant.
    pub name: String,
    /// The LLM model to use (e.g., "gpt-4", "llama3:8b").
    pub model: String,
    /// The role this participant is arguing.
    pub role: ParticipantRole,
    /// Optional custom system prompt override.
    pub custom_system_prompt: Option<String>,
    /// Voice ID for TTS (Phase 2).
    pub voice_id: Option<String>,
}

impl AIParticipant {
    /// Create a new participant with the given name, model, and role.
    pub fn new(name: impl Into<String>, model: impl Into<String>, role: ParticipantRole) -> Self {
        Self {
            name: name.into(),
            model: model.into(),
            role,
            custom_system_prompt: None,
            voice_id: None,
        }
    }

    /// Set a custom system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.custom_system_prompt = Some(prompt.into());
        self
    }

    /// Set the voice ID for TTS.
    pub fn with_voice(mut self, voice_id: impl Into<String>) -> Self {
        self.voice_id = Some(voice_id.into());
        self
    }

    /// Get the full display name with role.
    pub fn display_name_with_role(&self) -> String {
        format!("{} ({})", self.name, self.role.display_name())
    }
}
