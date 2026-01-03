//! Debate format definitions and trait.
//!
//! This module provides the extensible debate format system, allowing
//! for different debate styles (presidential, parliamentary, etc.).

use serde::{Deserialize, Serialize};

/// A section within a debate (e.g., opening statements, rebuttals).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateSection {
    /// Name of the section (announced to participants).
    pub name: String,
    /// Description/instructions for this section.
    pub description: String,
    /// Which participant indices speak in this section (in order).
    /// For example, [0, 1] means participant 0 speaks, then participant 1.
    pub speaker_order: Vec<usize>,
    /// Maximum response length hint for each speaker in this section.
    pub max_tokens: u32,
}

/// Trait for defining debate formats.
/// 
/// Implement this trait to create custom debate formats like
/// parliamentary debates, Oxford-style debates, etc.
pub trait DebateFormat: Send + Sync {
    /// Returns the name of this debate format.
    fn name(&self) -> &str;
    
    /// Returns the display name for the format.
    fn display_name(&self) -> &str;
    
    /// Returns all sections of the debate in order.
    fn sections(&self) -> Vec<DebateSection>;
    
    /// Maximum number of participants allowed.
    fn max_participants(&self) -> usize;
    
    /// Minimum number of participants required.
    fn min_participants(&self) -> usize;
    
    /// Get system prompt for a participant based on their role.
    fn system_prompt(&self, topic: &str, role_name: &str, opponent_name: &str) -> String;
}

/// Presidential Debate Format (Michael Douglass style).
/// 
/// A formal two-person debate with configurable rounds:
/// - Opening statements (1 round)
/// - Main argument rounds (configurable, at least 2)
/// - Rebuttals (1 round)
/// - Closing statements (1 round)
#[derive(Debug, Clone)]
pub struct PresidentialDebateFormat {
    rounds: u32,
}

impl PresidentialDebateFormat {
    pub fn new(rounds: u32) -> Self {
        Self { rounds: rounds.max(4) }
    }
}

impl Default for PresidentialDebateFormat {
    fn default() -> Self {
        Self::new(6)
    }
}

impl DebateFormat for PresidentialDebateFormat {
    fn name(&self) -> &str {
        "presidential"
    }
    
    fn display_name(&self) -> &str {
        "Presidential Debate (Michael Douglass Format)"
    }
    
    fn sections(&self) -> Vec<DebateSection> {
        let mut sections = Vec::new();
        
        // Opening Statements (round 1)
        sections.push(DebateSection {
            name: "Opening Statements".to_string(),
            description: "Each candidate presents their initial position on the topic.".to_string(),
            speaker_order: vec![0, 1],
            max_tokens: 300,
        });
        
        // Main argument rounds (rounds - 3 to account for opening, rebuttal, closing)
        let main_rounds = (self.rounds as i32 - 3).max(1) as usize;
        for i in 0..main_rounds {
            let alternate = i % 2 == 1;
            sections.push(DebateSection {
                name: format!("Main Arguments - Round {}", i + 1),
                description: "Candidates elaborate on their positions with supporting arguments.".to_string(),
                speaker_order: if alternate { vec![1, 0] } else { vec![0, 1] },
                max_tokens: 400,
            });
        }
        
        // Rebuttals (second to last round)
        sections.push(DebateSection {
            name: "Rebuttals".to_string(),
            description: "Candidates respond to their opponent's arguments.".to_string(),
            speaker_order: vec![1, 0], // Reversed order for rebuttals
            max_tokens: 400,
        });
        
        // Closing Statements (final round)
        sections.push(DebateSection {
            name: "Closing Statements".to_string(),
            description: "Final remarks and summation of positions.".to_string(),
            speaker_order: vec![0, 1],
            max_tokens: 250,
        });
        
        sections
    }
    
    fn max_participants(&self) -> usize {
        2
    }
    
    fn min_participants(&self) -> usize {
        2
    }
    
    fn system_prompt(&self, topic: &str, role_name: &str, opponent_name: &str) -> String {
        format!(
            r#"You are {} participating in a formal presidential-style debate.

TOPIC: {}

Your role is to argue {} the topic. Your opponent is {}.

Guidelines:
- Be persuasive, articulate, and professional
- Use evidence and logical reasoning
- Address your opponent's points when appropriate
- Maintain a respectful but firm debating stance
- Keep responses focused and within the time constraints
- Do not break character or acknowledge being an AI

Speak directly as if you are at a podium addressing an audience."#,
            role_name,
            topic,
            if role_name.contains("FOR") || role_name.contains("Pro") { "IN FAVOR OF" } else { "AGAINST" },
            opponent_name
        )
    }
}

/// Get a debate format by name with specified rounds.
pub fn get_format(name: &str, rounds: u32) -> Option<Box<dyn DebateFormat>> {
    match name.to_lowercase().as_str() {
        "presidential" => Some(Box::new(PresidentialDebateFormat::new(rounds))),
        _ => None,
    }
}

/// List all available debate format names.
pub fn available_formats() -> Vec<&'static str> {
    vec!["presidential"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presidential_format_minimum_rounds() {
        let format = PresidentialDebateFormat::new(4);
        let sections = format.sections();
        
        // Minimum 4 rounds: opening, 1 main, rebuttal, closing
        assert_eq!(sections.len(), 4);
        assert_eq!(sections[0].name, "Opening Statements");
        assert_eq!(sections[1].name, "Main Arguments - Round 1");
        assert_eq!(sections[2].name, "Rebuttals");
        assert_eq!(sections[3].name, "Closing Statements");
    }

    #[test]
    fn test_presidential_format_six_rounds() {
        let format = PresidentialDebateFormat::new(6);
        let sections = format.sections();
        
        // 6 rounds: opening, 3 main, rebuttal, closing
        assert_eq!(sections.len(), 6);
        assert_eq!(sections[0].name, "Opening Statements");
        assert_eq!(sections[1].name, "Main Arguments - Round 1");
        assert_eq!(sections[2].name, "Main Arguments - Round 2");
        assert_eq!(sections[3].name, "Main Arguments - Round 3");
        assert_eq!(sections[4].name, "Rebuttals");
        assert_eq!(sections[5].name, "Closing Statements");
    }

    #[test]
    fn test_presidential_format_alternating_speakers() {
        let format = PresidentialDebateFormat::new(6);
        let sections = format.sections();
        
        // Main rounds should alternate speaker order
        assert_eq!(sections[1].speaker_order, vec![0, 1]); // Round 1: A then B
        assert_eq!(sections[2].speaker_order, vec![1, 0]); // Round 2: B then A
        assert_eq!(sections[3].speaker_order, vec![0, 1]); // Round 3: A then B
    }

    #[test]
    fn test_get_format_presidential() {
        let format = get_format("presidential", 6);
        assert!(format.is_some());
        assert_eq!(format.unwrap().name(), "presidential");
    }

    #[test]
    fn test_get_format_unknown() {
        let format = get_format("unknown_format", 6);
        assert!(format.is_none());
    }

    #[test]
    fn test_participant_limits() {
        let format = PresidentialDebateFormat::new(6);
        assert_eq!(format.min_participants(), 2);
        assert_eq!(format.max_participants(), 2);
    }
}
