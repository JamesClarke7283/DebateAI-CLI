//! Debate orchestration logic.
//!
//! Manages the debate flow, API calls, and message history.

use crate::debate_format::{DebateFormat, DebateSection};
use crate::error::DebateError;
use crate::participant::AIParticipant;

use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestUserMessage,
    CreateChatCompletionRequestArgs,
};
use serde::{Deserialize, Serialize};

/// Configuration for running a debate.
#[derive(Debug, Clone)]
pub struct DebateConfig {
    /// The topic being debated.
    pub topic: String,
    /// OpenAI-compatible API base URL.
    pub api_base: String,
    /// API key for authentication.
    pub api_key: String,
}

impl DebateConfig {
    pub fn new(
        topic: impl Into<String>,
        api_base: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            topic: topic.into(),
            api_base: api_base.into(),
            api_key: api_key.into(),
        }
    }
}

/// A message in the debate transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateMessage {
    /// Section name when this was spoken.
    pub section: String,
    /// Index of the speaker (into participants array).
    pub speaker_index: usize,
    /// Speaker's name.
    pub speaker_name: String,
    /// The content of the message.
    pub content: String,
}

/// Callback for debate events.
pub type DebateCallback = Box<dyn Fn(DebateEvent) + Send + Sync>;

/// Events emitted during a debate.
#[derive(Debug, Clone)]
pub enum DebateEvent {
    /// A new section is starting.
    SectionStart { name: String, description: String },
    /// A participant is about to speak.
    SpeakerStart { name: String, role: String },
    /// A participant has finished speaking.
    SpeakerMessage { name: String, content: String },
    /// The debate has concluded.
    DebateEnd,
}

/// Orchestrates the debate between AI participants.
pub struct DebateOrchestrator {
    config: DebateConfig,
    participants: Vec<AIParticipant>,
    format: Box<dyn DebateFormat>,
    /// Message history per participant (for context).
    histories: Vec<Vec<ChatCompletionRequestMessage>>,
    /// Full debate transcript.
    transcript: Vec<DebateMessage>,
    /// Event callback.
    callback: Option<DebateCallback>,
}

impl DebateOrchestrator {
    /// Create a new orchestrator with the given configuration.
    pub fn new(
        config: DebateConfig,
        participants: Vec<AIParticipant>,
        format: Box<dyn DebateFormat>,
    ) -> Result<Self, DebateError> {
        let participant_count = participants.len();
        let min = format.min_participants();
        let max = format.max_participants();

        if participant_count < min || participant_count > max {
            return Err(DebateError::InvalidParticipantCount {
                min,
                max,
                actual: participant_count,
            });
        }

        let histories = participants
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let opponent_idx = if i == 0 { 1 } else { 0 };
                let opponent_name = participants
                    .get(opponent_idx)
                    .map(|op| op.name.as_str())
                    .unwrap_or("Opponent");

                let system_prompt = p.custom_system_prompt.clone().unwrap_or_else(|| {
                    format.system_prompt(&config.topic, &p.display_name_with_role(), opponent_name)
                });

                vec![ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessage {
                        content: system_prompt.into(),
                        name: None,
                    },
                )]
            })
            .collect();

        Ok(Self {
            config,
            participants,
            format,
            histories,
            transcript: Vec::new(),
            callback: None,
        })
    }

    /// Set a callback for debate events.
    pub fn with_callback(mut self, callback: DebateCallback) -> Self {
        self.callback = Some(callback);
        self
    }

    /// Run the full debate.
    pub async fn run(&mut self) -> Result<Vec<DebateMessage>, DebateError> {
        let sections = self.format.sections();

        for section in sections {
            self.run_section(&section).await?;
        }

        self.emit_event(DebateEvent::DebateEnd);
        Ok(self.transcript.clone())
    }

    /// Run a single debate section.
    async fn run_section(&mut self, section: &DebateSection) -> Result<(), DebateError> {
        self.emit_event(DebateEvent::SectionStart {
            name: section.name.clone(),
            description: section.description.clone(),
        });

        for &speaker_idx in &section.speaker_order {
            if speaker_idx >= self.participants.len() {
                continue;
            }

            let participant = &self.participants[speaker_idx];
            self.emit_event(DebateEvent::SpeakerStart {
                name: participant.name.clone(),
                role: participant.role.display_name().to_string(),
            });

            // Build the prompt for this turn
            let section_prompt = format!(
                "[{} - {}]\nPlease provide your {}.",
                section.name,
                section.description,
                section.name.to_lowercase()
            );

            // Add section prompt to this participant's history
            self.histories[speaker_idx].push(ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessage {
                    content: section_prompt.into(),
                    name: None,
                },
            ));

            // Get response from the AI with retry logic for empty responses
            let max_empty_retries = 3;
            let mut sanitized_response = String::new();

            for attempt in 0..max_empty_retries {
                let response = self.get_completion(speaker_idx, section.max_tokens).await?;
                sanitized_response = sanitize_response(&response);

                // Check if response is non-empty (has meaningful content)
                if !sanitized_response.trim().is_empty() && sanitized_response.trim().len() > 10 {
                    break;
                }

                // Log retry attempt (response was empty or too short)
                if attempt < max_empty_retries - 1 {
                    eprintln!(
                        "  [Retry {}/{}] Empty response from {}, retrying...",
                        attempt + 1,
                        max_empty_retries,
                        participant.name
                    );
                    // Brief delay before retry
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }

            // If still empty after retries, return an error
            if sanitized_response.trim().is_empty() || sanitized_response.trim().len() <= 10 {
                return Err(DebateError::ConfigError(format!(
                    "AI participant '{}' returned empty response after {} retries. Debate cannot continue.",
                    participant.name, max_empty_retries
                )));
            }

            // Record the message
            let message = DebateMessage {
                section: section.name.clone(),
                speaker_index: speaker_idx,
                speaker_name: participant.name.clone(),
                content: sanitized_response.clone(),
            };
            self.transcript.push(message);

            self.emit_event(DebateEvent::SpeakerMessage {
                name: participant.name.clone(),
                content: sanitized_response.clone(),
            });

            // Add assistant response to speaker's history
            self.histories[speaker_idx].push(ChatCompletionRequestMessage::Assistant(
                ChatCompletionRequestAssistantMessage {
                    content: Some(sanitized_response.clone().into()),
                    name: None,
                    tool_calls: None,
                    refusal: None,
                    audio: None,
                    function_call: None,
                },
            ));

            // Add opponent's statement to all other participants' histories
            for (i, history) in self.histories.iter_mut().enumerate() {
                if i != speaker_idx {
                    let opponent_msg = format!(
                        "[Opponent {} said]: {}",
                        self.participants[speaker_idx].name, sanitized_response
                    );
                    history.push(ChatCompletionRequestMessage::User(
                        ChatCompletionRequestUserMessage {
                            content: opponent_msg.into(),
                            name: None,
                        },
                    ));
                }
            }
        }

        Ok(())
    }

    /// Get a completion from the AI for a specific participant.
    /// Includes retry logic with exponential backoff for resilience.
    async fn get_completion(
        &self,
        participant_idx: usize,
        max_tokens: u32,
    ) -> Result<String, DebateError> {
        let participant = &self.participants[participant_idx];
        let history = &self.histories[participant_idx];

        // Create custom HTTP client that skips SSL verification with timeout
        let http_client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| {
                DebateError::ConfigError(format!("Failed to create HTTP client: {}", e))
            })?;

        let config = OpenAIConfig::new()
            .with_api_key(&self.config.api_key)
            .with_api_base(&self.config.api_base);

        let client = Client::with_config(config).with_http_client(http_client);

        let request = CreateChatCompletionRequestArgs::default()
            .model(&participant.model)
            .max_completion_tokens(max_tokens)
            .messages(history.clone())
            .build()?;

        // Retry logic with exponential backoff
        let max_retries = 3;
        let mut last_error = None;

        for attempt in 0..max_retries {
            if attempt > 0 {
                // Exponential backoff: 1s, 2s, 4s
                let delay = std::time::Duration::from_secs(1 << attempt);
                tokio::time::sleep(delay).await;
            }

            match client.chat().create(request.clone()).await {
                Ok(response) => {
                    let content = response
                        .choices
                        .first()
                        .and_then(|c| c.message.content.clone())
                        .unwrap_or_default();
                    return Ok(content);
                }
                Err(e) => {
                    last_error = Some(e);
                    // Only retry on transient errors
                    if attempt < max_retries - 1 {
                        continue;
                    }
                }
            }
        }

        Err(last_error.map(DebateError::from).unwrap_or_else(|| {
            DebateError::ConfigError("Unknown API error after retries".to_string())
        }))
    }

    /// Emit an event if a callback is registered.
    fn emit_event(&self, event: DebateEvent) {
        if let Some(ref callback) = self.callback {
            callback(event);
        }
    }

    /// Get the full transcript.
    pub fn transcript(&self) -> &[DebateMessage] {
        &self.transcript
    }

    /// Get participants.
    pub fn participants(&self) -> &[AIParticipant] {
        &self.participants
    }
}

/// Sanitize AI response by stripping reasoning tokens and XML-like tags.
///
/// Removes patterns like <thinking>...</thinking>, <reflection>...</reflection>, etc.
fn sanitize_response(response: &str) -> String {
    // List of known reasoning/internal tags to strip with their content
    let tags_to_strip = [
        "thinking",
        "think",
        "reflection",
        "reflect",
        "internal",
        "reasoning",
        "thought",
        "scratch",
        "scratchpad",
        "plan",
        "analysis",
        "analyze",
        "consider",
        "pondering",
        "deliberation",
    ];

    let mut result = response.to_string();

    // Strip each known tag and its content
    for tag in &tags_to_strip {
        // Match <tag>...</tag> including with attributes and newlines
        let pattern = format!(r"(?is)<{tag}[^>]*>.*?</{tag}>", tag = tag);
        if let Ok(re) = regex::Regex::new(&pattern) {
            result = re.replace_all(&result, "").to_string();
        }
    }

    // Also remove any remaining orphaned opening/closing tags
    if let Ok(orphan_re) = regex::Regex::new(r"</?[\w]+[^>]*>") {
        result = orphan_re.replace_all(&result, "").to_string();
    }

    // Remove markdown emphasis markers (asterisks)
    result = result.replace("*", "");

    // Clean up extra whitespace (multiple spaces/newlines become single)
    if let Ok(ws_re) = regex::Regex::new(r"\s+") {
        result = ws_re.replace_all(&result, " ").to_string();
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_response_thinking_tags() {
        let input = "<thinking>Let me think about this...</thinking>The answer is 42.";
        let output = sanitize_response(input);
        assert_eq!(output, "The answer is 42.");
    }

    #[test]
    fn test_sanitize_response_reflection_tags() {
        let input = "Hello <reflection>internal thought</reflection> world!";
        let output = sanitize_response(input);
        assert_eq!(output, "Hello world!");
    }

    #[test]
    fn test_sanitize_response_no_tags() {
        let input = "No tags here, just text.";
        let output = sanitize_response(input);
        assert_eq!(output, "No tags here, just text.");
    }

    #[test]
    fn test_sanitize_response_multiline_tags() {
        let input = "<thinking>\nMultiple\nlines\nof\nthought\n</thinking>Final answer here.";
        let output = sanitize_response(input);
        assert_eq!(output, "Final answer here.");
    }

    #[test]
    fn test_sanitize_response_nested_content() {
        let input = "Start <think>nested <inner>tags</inner> content</think> end";
        let output = sanitize_response(input);
        // After stripping <think> and orphan tags, should get clean result
        assert!(!output.contains("<"));
        assert!(!output.contains(">"));
    }

    #[test]
    fn test_sanitize_response_multiple_tag_types() {
        let input = "<plan>First plan</plan>Then <reasoning>reason</reasoning> finally the answer.";
        let output = sanitize_response(input);
        assert_eq!(output, "Then finally the answer.");
    }
}
