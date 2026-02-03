//! SMILE Loop CLI
//!
//! Main entry point for running SMILE Loop against tutorials.

use clap::Parser;
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
    tutorial: String,

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

// Allow unnecessary_wraps: main will return errors once orchestration is implemented
#[allow(clippy::unnecessary_wraps)]
fn main() -> anyhow::Result<()> {
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
    tracing::debug!(tutorial = %args.tutorial, "Tutorial file");
    tracing::debug!(config = ?args.config, "Config file");
    tracing::debug!(output_dir = ?args.output_dir, "Output directory");

    // Print parsed arguments for verification
    println!("Tutorial: {}", args.tutorial);
    if let Some(ref config) = args.config {
        println!("Config: {config}");
    }
    if let Some(ref output_dir) = args.output_dir {
        println!("Output: {output_dir}");
    }

    // TODO: Actual orchestration will be implemented in Phase 8
    // This will involve:
    // - Loading and validating the tutorial file
    // - Parsing the configuration
    // - Setting up the container environment
    // - Running the Student/Mentor loop
    // - Generating reports

    Ok(())
}
