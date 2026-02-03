//! SMILE Loop CLI
//!
//! Main entry point for running SMILE Loop against tutorials.

use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use smile_orchestrator::{Config, Tutorial};
use tracing_subscriber::EnvFilter;

/// SMILE Loop - Tutorial Validation Tool
///
/// Validates technical tutorials by simulating a constrained learner (Student agent)
/// that attempts to follow instructions, escalating to a Mentor agent when stuck.
#[derive(Parser, Debug)]
#[command(name = "smile")]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the tutorial markdown file
    #[arg(value_name = "TUTORIAL")]
    tutorial: Option<String>,

    /// Path to configuration file (default: smile.json in current directory)
    #[arg(short, long, value_name = "FILE")]
    config: Option<String>,

    /// Output directory for reports
    #[arg(short, long, value_name = "DIR")]
    output_dir: Option<String>,

    /// Enable verbose output (sets log level to debug)
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> ExitCode {
    // Parse CLI arguments
    let args = Args::parse();

    // Initialize tracing subscriber with appropriate filter
    // Priority: RUST_LOG env var > --verbose flag > default (info)
    let filter = if args.verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!("SMILE Loop starting");
    tracing::debug!(config = ?args.config, "Config file");
    tracing::debug!(output_dir = ?args.output_dir, "Output directory");

    // Load configuration
    let mut config = match load_config(args.config.as_deref()) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    // Apply CLI argument overrides
    if let Some(ref tutorial) = args.tutorial {
        config.tutorial.clone_from(tutorial);
    }
    if let Some(ref output_dir) = args.output_dir {
        config.output_dir.clone_from(output_dir);
    }

    // Re-validate after overrides (in case overrides introduced invalid values)
    if let Err(e) = config.validate() {
        eprintln!("Error: {e}");
        return ExitCode::from(1);
    }

    tracing::debug!(tutorial = %config.tutorial, "Tutorial file");
    tracing::debug!(output_dir = %config.output_dir, "Output directory");
    tracing::debug!(llm_provider = ?config.llm_provider, "LLM provider");
    tracing::debug!(max_iterations = config.max_iterations, "Max iterations");

    // Print loaded config values for verification
    println!("Configuration loaded:");
    println!("  Tutorial: {}", config.tutorial);
    println!("  Output directory: {}", config.output_dir);
    println!("  LLM provider: {:?}", config.llm_provider);
    println!("  Max iterations: {}", config.max_iterations);
    println!("  Timeout: {}s", config.timeout);
    println!("  Container image: {}", config.container_image);

    // Load tutorial file with images
    tracing::info!(tutorial = %config.tutorial, "Loading tutorial");
    let tutorial = match Tutorial::load_with_images(&config.tutorial) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    // Print tutorial info
    println!();
    println!("Tutorial loaded:");
    println!("  Path: {}", tutorial.path.display());
    println!("  Size: {} bytes", tutorial.size_bytes);
    println!("  Images: {}", tutorial.images.len());

    for img in &tutorial.images {
        tracing::debug!(
            reference = %img.reference,
            format = %img.format,
            size = img.data.len(),
            "Image loaded"
        );
    }

    // TODO: Actual orchestration will be implemented in Phase 8
    // This will involve:
    // - Setting up the container environment
    // - Running the Student/Mentor loop
    // - Generating reports

    ExitCode::SUCCESS
}

/// Loads configuration from the specified path or default location.
///
/// If a config path is explicitly provided via `--config`, the file must exist.
/// Otherwise, loads from `smile.json` in the current directory (or defaults if not found).
fn load_config(config_path: Option<&str>) -> anyhow::Result<Config> {
    match config_path {
        Some(path_str) => {
            let path = Path::new(path_str);
            // When --config is explicitly provided, the file must exist
            if !path.exists() {
                anyhow::bail!(
                    "Config file not found: '{}'\n\nSuggestion: Check the path or remove the --config flag to use defaults",
                    path.display()
                );
            }
            Config::load_from_file(path).map_err(|e| anyhow::anyhow!("{e}"))
        }
        None => {
            // Load from current directory's smile.json or use defaults
            Config::load().map_err(|e| anyhow::anyhow!("{e}"))
        }
    }
}
