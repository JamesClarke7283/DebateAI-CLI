//! DebateAI CLI - AI Debate Tool
//!
//! A command-line tool for running AI debates between multiple LLM participants.

use clap::{ArgAction, Parser};
use colored::Colorize;
use debateai_core::{
    debate_format, AIParticipant, Config, DebateConfig, DebateEvent, DebateOrchestrator,
    DebateTts, ParticipantRole, VoicesConfig, combine_audio_segments, generate_output_filename,
};
use std::env;
use std::path::PathBuf;

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

    /// Output directory for audio files (default: current directory)
    #[arg(short, long, default_value = ".", value_name = "DIR")]
    output_dir: PathBuf,

    /// Disable audio output (text-only mode)
    #[arg(long)]
    disable_audio: bool,

    /// Maximum reasoning tokens for models (0 = model default, -1 = unlimited)
    #[arg(long, default_value = "8192", value_name = "TOKENS")]
    reasoning_tokens: i32,

    /// Path to custom config.toml file
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Voice IDs for participants (specify in order: FOR, AGAINST)
    /// Examples: bf_emma, bm_george, af_sky, am_adam
    #[arg(long, action = ArgAction::Append, value_name = "VOICE")]
    voice: Vec<String>,

    /// Announcer voice ID (for section announcements in audio)
    #[arg(long, value_name = "VOICE")]
    announcer_voice: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env file if present
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    // Load configuration
    let mut config = if let Some(config_path) = &cli.config {
        Config::load(config_path)?
    } else if PathBuf::from("config.toml").exists() {
        Config::load("config.toml")?
    } else {
        debateai_core::config::default_config()
    };

    // Override voices from CLI if provided
    if let Some(for_voice) = cli.voice.first() {
        config.voices.for_voice = for_voice.clone();
    }
    if let Some(against_voice) = cli.voice.get(1) {
        config.voices.against_voice = against_voice.clone();
    }
    if let Some(announcer) = &cli.announcer_voice {
        config.voices.announcer_voice = announcer.clone();
    }

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
    let rounds = cli.rounds.max(4);
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

    // Create participants with voices from config
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
            let voice = config.get_voice(role == ParticipantRole::For).to_string();
            AIParticipant::new(name, model.clone(), role).with_voice(voice)
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
    
    if !cli.disable_audio {
        println!();
        println!("{} {}", "Audio Output:".bold(), cli.output_dir.display().to_string().bright_green());
    }
    
    println!();
    println!("{}", "─".repeat(70).dimmed());

    // Create debate configuration
    let debate_config = DebateConfig::new(&cli.topic, api_base, api_key);

    // Create orchestrator with event callback
    let transcript_clone = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let transcript_for_callback = transcript_clone.clone();
    
    let callback = create_console_callback(transcript_for_callback);
    let mut orchestrator = DebateOrchestrator::new(debate_config, participants.clone(), format)?
        .with_callback(callback);

    // Run the debate
    let transcript = orchestrator.run().await?;

    println!();
    println!("{}", "═".repeat(70).bright_blue());
    println!("{}", "  Debate concluded.".bright_green().bold());
    println!("{}", "═".repeat(70).bright_blue());

    // Generate TTS output unless disabled
    if !cli.disable_audio {
        println!();
        println!("{}", "Generating audio output...".bright_yellow());
        
        // Create output directory if needed
        std::fs::create_dir_all(&cli.output_dir)?;
        
        // Initialize TTS engine
        match DebateTts::new(config.voices.clone()).await {
            Ok(mut tts) => {
                // Synthesize each message with graceful degradation
                let mut audio_segments = Vec::new();
                let mut failed_segments = 0;
                
                for message in &transcript {
                    let role = &participants[message.speaker_index].role;
                    print!("  Synthesizing {} ({})...", message.speaker_name.bright_cyan(), message.section);
                    std::io::Write::flush(&mut std::io::stdout())?;
                    
                    match tts.synthesize_message(message, role) {
                        Ok(audio) => {
                            audio_segments.push(audio);
                            println!(" {}", "✓".bright_green());
                        }
                        Err(e) => {
                            failed_segments += 1;
                            println!(" {} ({})", "✗".bright_red(), e);
                            // Add silence instead of failing completely
                            audio_segments.push(vec![0.0; 24000]); // 1 second of silence
                        }
                    }
                }
                
                if failed_segments > 0 {
                    println!("{}", format!("  Warning: {} segment(s) failed to synthesize", failed_segments).yellow());
                }
                
                if !audio_segments.is_empty() {
                    // Combine with gaps between speakers
                    println!("  Combining audio segments...");
                    let combined = combine_audio_segments(audio_segments, 1.0, 24000);
                    
                    // Save to file
                    let filename = generate_output_filename(&cli.topic);
                    let output_path = cli.output_dir.join(&filename);
                    
                    match tts.save_wav(&output_path, &combined) {
                        Ok(_) => {
                            println!();
                            println!(
                                "{} {}",
                                "Audio saved:".bright_green().bold(),
                                output_path.display().to_string().bright_white()
                            );
                        }
                        Err(e) => {
                            println!();
                            println!("{} {}", "Failed to save audio:".red().bold(), e);
                        }
                    }
                }
            }
            Err(e) => {
                println!("{} {}", "TTS initialization failed:".red().bold(), e);
                println!("{}", "Skipping audio generation. Debate transcript completed successfully.".yellow());
            }
        }
    }

    println!();

    Ok(())
}

/// Create a callback that prints debate events to the console.
fn create_console_callback(
    _transcript: std::sync::Arc<std::sync::Mutex<Vec<debateai_core::DebateMessage>>>
) -> Box<dyn Fn(DebateEvent) + Send + Sync> {
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
