//! TTS module for text-to-speech synthesis using kokoro-tiny.

use kokoro_tiny::TtsEngine;
use std::path::Path;

use crate::config::VoicesConfig;
use crate::error::DebateError;
use crate::orchestrator::DebateMessage;
use crate::participant::ParticipantRole;

/// Audio segment from TTS synthesis.
pub struct AudioSegment {
    /// Raw audio samples.
    pub samples: Vec<f32>,
    /// Speaker name for this segment.
    pub speaker: String,
    /// Voice ID used.
    pub voice_id: String,
}

/// TTS synthesizer for debate output.
pub struct DebateTts {
    engine: TtsEngine,
    voices: VoicesConfig,
    available_voices: Vec<String>,
}

impl DebateTts {
    /// Initialize the TTS engine (downloads model on first run).
    pub async fn new(voices: VoicesConfig) -> Result<Self, DebateError> {
        let engine = TtsEngine::new()
            .await
            .map_err(|e| DebateError::TtsError(format!("Failed to initialize TTS: {}", e)))?;

        let available_voices = engine.voices();

        Ok(Self {
            engine,
            voices,
            available_voices,
        })
    }

    /// Get list of available voice IDs.
    pub fn available_voices(&self) -> &[String] {
        &self.available_voices
    }

    /// Validate that a voice ID exists.
    pub fn validate_voice(&self, voice_id: &str) -> Result<(), DebateError> {
        if voice_id.is_empty() {
            return Err(DebateError::TtsError(format!(
                "Voice ID cannot be empty. Available voices:\n{}",
                self.format_available_voices()
            )));
        }

        if !self.available_voices.contains(&voice_id.to_string()) {
            return Err(DebateError::TtsError(format!(
                "Unknown voice '{}'. Available voices:\n{}",
                voice_id,
                self.format_available_voices()
            )));
        }

        Ok(())
    }

    /// Format available voices for display.
    fn format_available_voices(&self) -> String {
        let mut english_voices: Vec<&String> = self
            .available_voices
            .iter()
            .filter(|v| {
                v.starts_with("af_")
                    || v.starts_with("am_")
                    || v.starts_with("bf_")
                    || v.starts_with("bm_")
            })
            .collect();
        english_voices.sort();

        english_voices
            .iter()
            .map(|v| format!("  - {}", v))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Validate all configured voices.
    pub fn validate_all_voices(&self) -> Result<(), DebateError> {
        self.validate_voice(&self.voices.for_voice)?;
        self.validate_voice(&self.voices.against_voice)?;
        self.validate_voice(&self.voices.announcer_voice)?;
        Ok(())
    }

    /// Synthesize text in chunks to handle long text.
    /// Kokoro-tiny has a strict limit on text length, so we split into small chunks.
    pub fn synthesize(&mut self, text: &str, voice_id: &str) -> Result<Vec<f32>, DebateError> {
        // Validate voice first
        self.validate_voice(voice_id)?;

        // Split text into small chunks (kokoro has ~200 char safe limit)
        let chunks = split_into_chunks(text, 200);

        let mut all_samples = Vec::new();

        for chunk in chunks {
            if chunk.trim().is_empty() {
                continue;
            }

            let samples = self
                .engine
                .synthesize(&chunk, Some(voice_id))
                .map_err(|e| DebateError::TtsError(format!("Synthesis failed: {}", e)))?;

            all_samples.extend(samples);

            // Add pause between chunks (0.3 seconds at 24kHz) to prevent cutoff
            all_samples.extend(vec![0.0; 7200]);
        }

        // Add trailing padding (0.5 seconds) at end of entire message to prevent final cutoff
        all_samples.extend(vec![0.0; 12000]);

        Ok(all_samples)
    }

    /// Synthesize an announcer segment.
    pub fn synthesize_announcer(&mut self, text: &str) -> Result<Vec<f32>, DebateError> {
        let voice = self.voices.announcer_voice.clone();
        self.synthesize(text, &voice)
    }

    /// Synthesize a debate message based on speaker role.
    pub fn synthesize_message(
        &mut self,
        message: &DebateMessage,
        role: &ParticipantRole,
    ) -> Result<Vec<f32>, DebateError> {
        let voice_id = match role {
            ParticipantRole::For => self.voices.for_voice.clone(),
            ParticipantRole::Against => self.voices.against_voice.clone(),
            ParticipantRole::Neutral => self.voices.announcer_voice.clone(),
        };

        self.synthesize(&message.content, &voice_id)
    }

    /// Save audio samples to a WAV file.
    pub fn save_wav<P: AsRef<Path>>(&self, path: P, samples: &[f32]) -> Result<(), DebateError> {
        self.engine
            .save_wav(path.as_ref().to_str().unwrap_or("output.wav"), samples)
            .map_err(|e| DebateError::TtsError(format!("Failed to save WAV: {}", e)))
    }

    /// Get voice ID for a role.
    pub fn voice_for_role(&self, role: &ParticipantRole) -> &str {
        match role {
            ParticipantRole::For => &self.voices.for_voice,
            ParticipantRole::Against => &self.voices.against_voice,
            ParticipantRole::Neutral => &self.voices.announcer_voice,
        }
    }
}

/// Split text into chunks that are safe for TTS synthesis.
fn split_into_chunks(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    // Split by sentence-ending punctuation
    for sentence in text.split_inclusive(&['.', '!', '?', ';'][..]) {
        let sentence = sentence.trim();
        if sentence.is_empty() {
            continue;
        }

        if current_chunk.len() + sentence.len() > max_chars {
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.trim().to_string());
                current_chunk = String::new();
            }

            // If single sentence is too long, split by commas
            if sentence.len() > max_chars {
                for part in sentence.split_inclusive(',') {
                    if current_chunk.len() + part.len() > max_chars {
                        if !current_chunk.is_empty() {
                            chunks.push(current_chunk.trim().to_string());
                            current_chunk = String::new();
                        }
                    }
                    current_chunk.push_str(part);
                    current_chunk.push(' ');
                }
            } else {
                current_chunk.push_str(sentence);
                current_chunk.push(' ');
            }
        } else {
            current_chunk.push_str(sentence);
            current_chunk.push(' ');
        }
    }

    if !current_chunk.trim().is_empty() {
        chunks.push(current_chunk.trim().to_string());
    }

    chunks
}

/// Adjust audio playback speed using linear interpolation.
/// Rate < 1.0 = slower (e.g., 0.75 = 75% speed), Rate > 1.0 = faster.
pub fn adjust_audio_speed(samples: Vec<f32>, rate: f32) -> Vec<f32> {
    if (rate - 1.0).abs() < 0.001 {
        return samples; // No change needed
    }

    // Calculate new length (slower = longer)
    let new_len = (samples.len() as f32 / rate) as usize;
    let mut result = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_pos = i as f32 * rate;
        let src_idx = src_pos as usize;
        let frac = src_pos - src_idx as f32;

        if src_idx + 1 < samples.len() {
            // Linear interpolation between adjacent samples
            let sample = samples[src_idx] * (1.0 - frac) + samples[src_idx + 1] * frac;
            result.push(sample);
        } else if src_idx < samples.len() {
            result.push(samples[src_idx]);
        }
    }

    result
}

/// Combine multiple audio segments with silence gaps.
pub fn combine_audio_segments(
    segments: Vec<Vec<f32>>,
    gap_seconds: f32,
    sample_rate: u32,
) -> Vec<f32> {
    let gap_samples = (gap_seconds * sample_rate as f32) as usize;
    let silence: Vec<f32> = vec![0.0; gap_samples];

    let mut combined = Vec::new();

    for (i, segment) in segments.into_iter().enumerate() {
        if i > 0 {
            combined.extend(&silence);
        }
        combined.extend(segment);
    }

    combined
}

/// Generate filename for debate output.
pub fn generate_output_filename(topic: &str) -> String {
    // Sanitize topic for filename
    let sanitized: String = topic
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    // Truncate if too long
    let truncated = if sanitized.len() > 50 {
        &sanitized[..50]
    } else {
        &sanitized
    };

    format!("DebateAI - {}.wav", truncated.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_output_filename() {
        assert_eq!(
            generate_output_filename("Should AI be open source?"),
            "DebateAI - Should AI be open source_.wav"
        );
    }

    #[test]
    fn test_generate_output_filename_long() {
        let long_topic = "A".repeat(100);
        let filename = generate_output_filename(&long_topic);
        assert!(filename.len() < 70);
    }

    #[test]
    fn test_combine_audio_segments() {
        let seg1 = vec![1.0, 1.0];
        let seg2 = vec![2.0, 2.0];
        let combined = combine_audio_segments(vec![seg1, seg2], 0.1, 10); // 1 sample gap at 10Hz

        assert_eq!(combined.len(), 5); // 2 + 1 gap + 2
        assert_eq!(combined[2], 0.0); // gap sample
    }

    #[test]
    fn test_split_into_chunks() {
        let text = "Hello world. This is a test. Another sentence here.";
        let chunks = split_into_chunks(text, 30);
        assert!(chunks.len() >= 1);
        for chunk in &chunks {
            assert!(chunk.len() <= 35); // Allow some flexibility
        }
    }
}
