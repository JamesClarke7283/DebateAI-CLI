//! Configuration module for loading TOML config files.

use serde::Deserialize;
use std::fs;
use std::path::Path;

use crate::error::DebateError;

/// Root configuration structure.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub debate: DebateFormatsConfig,
    pub voices: VoicesConfig,
    pub prompts: PromptsConfig,
}

/// Configuration for all debate formats.
#[derive(Debug, Clone, Deserialize)]
pub struct DebateFormatsConfig {
    pub presidential: PresidentialConfig,
}

/// Configuration for presidential debate format.
#[derive(Debug, Clone, Deserialize)]
pub struct PresidentialConfig {
    pub name: String,
    pub display_name: String,
    pub min_participants: usize,
    pub max_participants: usize,
    #[serde(default)]
    pub sections: Vec<SectionConfig>,
}

/// Configuration for a debate section.
#[derive(Debug, Clone, Deserialize)]
pub struct SectionConfig {
    pub name: String,
    pub description: String,
    pub speaker_order: Vec<usize>,
    pub max_tokens: u32,
}

/// Voice configuration for TTS.
#[derive(Debug, Clone, Deserialize)]
pub struct VoicesConfig {
    pub for_voice: String,
    pub against_voice: String,
    pub announcer_voice: String,
}

impl Default for VoicesConfig {
    fn default() -> Self {
        Self {
            for_voice: "bf_emma".to_string(),
            against_voice: "bm_george".to_string(),
            announcer_voice: "af_sky".to_string(),
        }
    }
}

/// System prompts configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptsConfig {
    pub for_prompt: String,
    pub against_prompt: String,
    #[serde(default)]
    pub announcer_template: String,
}

impl Config {
    /// Load configuration from a TOML file.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, DebateError> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| DebateError::ConfigError(format!("Failed to read config: {}", e)))?;

        toml::from_str(&content)
            .map_err(|e| DebateError::ConfigError(format!("Failed to parse config: {}", e)))
    }

    /// Load configuration from string content.
    pub fn from_str(content: &str) -> Result<Self, DebateError> {
        toml::from_str(content)
            .map_err(|e| DebateError::ConfigError(format!("Failed to parse config: {}", e)))
    }

    /// Get the system prompt for a participant, with placeholders replaced.
    pub fn get_prompt(&self, is_for: bool, name: &str, topic: &str, opponent_name: &str) -> String {
        let template = if is_for {
            &self.prompts.for_prompt
        } else {
            &self.prompts.against_prompt
        };

        template
            .replace("{name}", name)
            .replace("{topic}", topic)
            .replace("{opponent_name}", opponent_name)
    }

    /// Get voice ID for a participant role.
    pub fn get_voice(&self, is_for: bool) -> &str {
        if is_for {
            &self.voices.for_voice
        } else {
            &self.voices.against_voice
        }
    }
}

/// Default configuration embedded in the binary.
pub fn default_config() -> Config {
    Config {
        debate: DebateFormatsConfig {
            presidential: PresidentialConfig {
                name: "presidential".to_string(),
                display_name: "Presidential Debate (Lincoln-Douglas Style)".to_string(),
                min_participants: 2,
                max_participants: 2,
                sections: vec![
                    SectionConfig {
                        name: "Opening Statements".to_string(),
                        description: "Each candidate presents their opening position.".to_string(),
                        speaker_order: vec![0, 1],
                        max_tokens: 400,
                    },
                    SectionConfig {
                        name: "Direct Response".to_string(),
                        description: "Candidates respond to opening arguments.".to_string(),
                        speaker_order: vec![1, 0],
                        max_tokens: 350,
                    },
                    SectionConfig {
                        name: "Policy Discussion".to_string(),
                        description: "Candidates present policy positions.".to_string(),
                        speaker_order: vec![0, 1],
                        max_tokens: 400,
                    },
                    SectionConfig {
                        name: "Cross-Examination".to_string(),
                        description: "Candidates challenge each other's positions.".to_string(),
                        speaker_order: vec![1, 0],
                        max_tokens: 350,
                    },
                    SectionConfig {
                        name: "Final Rebuttals".to_string(),
                        description: "Candidates address opponent's strongest points.".to_string(),
                        speaker_order: vec![0, 1],
                        max_tokens: 350,
                    },
                    SectionConfig {
                        name: "Closing Statements".to_string(),
                        description: "Final appeals to the audience.".to_string(),
                        speaker_order: vec![1, 0],
                        max_tokens: 300,
                    },
                ],
            },
        },
        voices: VoicesConfig::default(),
        prompts: PromptsConfig {
            for_prompt: DEFAULT_FOR_PROMPT.to_string(),
            against_prompt: DEFAULT_AGAINST_PROMPT.to_string(),
            announcer_template: "[DEBATE SECTION: {section_name}]\n{section_description}"
                .to_string(),
        },
    }
}

const DEFAULT_FOR_PROMPT: &str = r#"You are {name} participating in a formal presidential-style debate.

DEBATE TOPIC: {topic}

YOUR POSITION: You are arguing IN FAVOR of the topic.
YOUR OPPONENT: {opponent_name}

DEBATE RULES:
- Present clear, compelling arguments supported by evidence and reasoning
- Address the specific question or topic directly
- Acknowledge and counter your opponent's arguments respectfully
- Use rhetorical techniques: ethos (credibility), pathos (emotion), logos (logic)
- Maintain a professional, presidential tone throughout
- Do NOT acknowledge being an AI - stay fully in character

SPEAKING STYLE:
- Speak directly to the audience
- Use confident, measured language befitting a presidential candidate
- Structure your responses with clear opening, supporting points, and conclusion

CRITICAL OUTPUT RULES:
- Output ONLY your spoken words - no scene directions or stage actions
- Do NOT include any text in parentheses like "(Steps to podium)" or "(Leans forward)"
- Do NOT include narration, descriptions of gestures, movements, or tone
- Do NOT include asterisks for emphasis or any markdown formatting
- The announcer provides context - just deliver your argument directly
"#;

const DEFAULT_AGAINST_PROMPT: &str = r#"You are {name} participating in a formal presidential-style debate.

DEBATE TOPIC: {topic}

YOUR POSITION: You are arguing AGAINST the topic.
YOUR OPPONENT: {opponent_name}

DEBATE RULES:
- Present clear, compelling arguments supported by evidence and reasoning
- Address the specific question or topic directly
- Acknowledge and counter your opponent's arguments respectfully
- Use rhetorical techniques: ethos (credibility), pathos (emotion), logos (logic)
- Maintain a professional, presidential tone throughout
- Do NOT acknowledge being an AI - stay fully in character

SPEAKING STYLE:
- Speak directly to the audience
- Use confident, measured language befitting a presidential candidate
- Structure your responses with clear opening, supporting points, and conclusion

CRITICAL OUTPUT RULES:
- Output ONLY your spoken words - no scene directions or stage actions
- Do NOT include any text in parentheses like "(Steps to podium)" or "(Leans forward)"
- Do NOT include narration, descriptions of gestures, movements, or tone
- Do NOT include asterisks for emphasis or any markdown formatting
- The announcer provides context - just deliver your argument directly
"#;
