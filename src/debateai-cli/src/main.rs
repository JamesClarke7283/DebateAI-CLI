//! DebateAI CLI - AI Debate Tool
//!
//! A command-line tool for running AI debates between multiple LLM participants.

use clap::{ArgAction, Parser};
use colored::Colorize;
use debateai_core::{
    debate_format, AIParticipant, DebateConfig, DebateEvent, DebateOrchestrator, ParticipantRole,
};
use std::env;

#[derive(Parser)]
#[command(
    name = "debateai",
    version,
    about = "AI Debate Tool - Watch AIs debate topics",
    long_about = "A CLI tool for running debates between AI participants using OpenAI-compatible APIs."
)]
struct Cli {
    /// The topic to debate
    #[arg(value_name = "TOPIC")]
    topic: String,

    /// Model names for participants (specify once per participant)
    /// For presidential format, specify exactly 2 models: -m model1 -m model2
    #[arg(short, long, action = ArgAction::Append, value_name = "MODEL")]
    model: Vec<String>,

    /// Debate format to use
    #[arg(long, default_value = "presidential", value_name = "FORMAT")]
    debate_format: String,

    /// Names for the participants (optional, specify in same order as models)
    #[arg(long, action = ArgAction::Append, value_name = "NAME")]
    name: Vec<String>,

    /// Number of debate rounds (minimum 4)
    #[arg(short, long, default_value = "6", value_name = "ROUNDS")]
    rounds: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env file if present
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    // Get API configuration from environment
    let api_base = env::var("OPENAI_API_BASE")
        .or_else(|_| env::var("OPENAI_BASE_URL"))
        .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

    let api_key = env::var("OPENAI_API_KEY").unwrap_or_else(|_| {
        eprintln!(
            "{}",
            "Warning: OPENAI_API_KEY not set. API calls may fail.".yellow()
        );
        String::new()
    });

    // Validate rounds
    let rounds = cli.rounds.max(4); // Minimum of 4 rounds
    if cli.rounds < 4 {
        eprintln!(
            "{}",
            format!("Warning: Rounds increased to minimum of 4 (was {}).", cli.rounds).yellow()
        );
    }

    // Get the debate format
    let format = debate_format::get_format(&cli.debate_format, rounds).ok_or_else(|| {
        format!(
            "Unknown debate format: '{}'. Available formats: {}",
            cli.debate_format,
            debate_format::available_formats().join(", ")
        )
    })?;

    // Validate model count
    let min_participants = format.min_participants();
    let max_participants = format.max_participants();

    if cli.model.len() < min_participants || cli.model.len() > max_participants {
        eprintln!(
            "{} The '{}' format requires {} to {} models, but {} were provided.",
            "Error:".red().bold(),
            cli.debate_format,
            min_participants,
            max_participants,
            cli.model.len()
        );
        eprintln!(
            "Usage: debateai \"{}\" {}",
            cli.topic,
            (0..min_participants)
                .map(|i| format!("-m model{}", i + 1))
                .collect::<Vec<_>>()
                .join(" ")
        );
        std::process::exit(1);
    }

    // Create participants
    let default_names = vec![
        "Candidate A".to_string(),
        "Candidate B".to_string(),
        "Candidate C".to_string(),
        "Candidate D".to_string(),
    ];
    let roles = [
        ParticipantRole::For,
        ParticipantRole::Against,
        ParticipantRole::For,
        ParticipantRole::Against,
    ];

    let participants: Vec<AIParticipant> = cli
        .model
        .iter()
        .enumerate()
        .map(|(i, model)| {
            let name = cli
                .name
                .get(i)
                .cloned()
                .unwrap_or_else(|| default_names[i % default_names.len()].clone());
            let role = roles[i % roles.len()].clone();
            AIParticipant::new(name, model.clone(), role)
        })
        .collect();

    // Print header
    println!();
    println!("{}", "═".repeat(70).bright_blue());
    println!(
        "{}",
        format!("  {} - {}", "DebateAI".bold(), format.display_name())
            .bright_blue()
            .bold()
    );
    println!("{}", "═".repeat(70).bright_blue());
    println!();
    println!("{} {}", "Topic:".bold(), cli.topic.bright_white());
    println!();
    println!("{}", "Participants:".bold());
    for (i, p) in participants.iter().enumerate() {
        println!(
            "  {}. {} ({}) - using {}",
            i + 1,
            p.name.bright_cyan(),
            p.role.display_name().yellow(),
            p.model.dimmed()
        );
    }
    println!();
    println!("{}", "─".repeat(70).dimmed());

    // Create debate configuration
    let config = DebateConfig::new(&cli.topic, api_base, api_key);

    // Create orchestrator with event callback
    let callback = create_console_callback();
    let mut orchestrator = DebateOrchestrator::new(config, participants, format)?
        .with_callback(callback);

    // Run the debate
    let _transcript = orchestrator.run().await?;

    println!();
    println!("{}", "═".repeat(70).bright_blue());
    println!("{}", "  Debate concluded.".bright_green().bold());
    println!("{}", "═".repeat(70).bright_blue());
    println!();

    Ok(())
}

/// Create a callback that prints debate events to the console.
fn create_console_callback() -> Box<dyn Fn(DebateEvent) + Send + Sync> {
    Box::new(move |event| match event {
        DebateEvent::SectionStart { name, description } => {
            println!();
            println!("{}", "═".repeat(70).bright_magenta());
            println!(
                "{}",
                format!("  📢 ANNOUNCER: {}", name)
                    .bright_magenta()
                    .bold()
            );
            println!("  {}", description.dimmed());
            println!("{}", "═".repeat(70).bright_magenta());
            println!();
        }
        DebateEvent::SpeakerStart { name, role } => {
            println!(
                "{} {} {}",
                "▶".bright_cyan(),
                name.bright_cyan().bold(),
                format!("({})", role).yellow()
            );
        }
        DebateEvent::SpeakerMessage { name: _, content } => {
            // Word wrap and indent the content
            let wrapped = textwrap(&content, 66);
            for line in wrapped.lines() {
                println!("  {}", line);
            }
            println!();
        }
        DebateEvent::DebateEnd => {
            // Handled in main
        }
    })
}

/// Simple text wrapping function.
fn textwrap(text: &str, width: usize) -> String {
    let mut result = String::new();
    let mut current_line_len = 0;

    for word in text.split_whitespace() {
        if current_line_len + word.len() + 1 > width && current_line_len > 0 {
            result.push('\n');
            current_line_len = 0;
        }
        if current_line_len > 0 {
            result.push(' ');
            current_line_len += 1;
        }
        result.push_str(word);
        current_line_len += word.len();
    }

    result
}
