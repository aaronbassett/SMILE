//! SMILE Loop CLI
//!
//! Main entry point for running SMILE Loop against tutorials.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use smile_container::{ContainerManager, CreateContainerOptions, Mount};
use smile_orchestrator::{
    create_router, AppState, Config, EventBroadcaster, IterationRecord, LoopState, LoopStatus,
    MentorNote, StateLock, StudentStatus, Tutorial,
};
use smile_report::{
    json::JsonGenerator, IterationInput, MarkdownGenerator, MentorNoteInput, ReportGenerator,
    ReportInput, ReportStatus, StudentStatusInput,
};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing_subscriber::EnvFilter;

/// Default port for the HTTP API server.
const DEFAULT_PORT: u16 = 3000;

/// Poll interval for checking loop state changes (in milliseconds).
const POLL_INTERVAL_MS: u64 = 500;

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

    /// Port for the HTTP API server
    #[arg(short, long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// Resume from existing state instead of starting fresh
    #[arg(long)]
    resume: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
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

    // Run the main loop and handle errors
    match run_smile_loop(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::from(1)
        }
    }
}

/// Runs the main SMILE loop.
///
/// This function orchestrates the entire validation process:
/// 1. Load config and tutorial
/// 2. Check Docker availability
/// 3. Acquire state lock
/// 4. Load or create state
/// 5. Setup container
/// 6. Start HTTP server
/// 7. Run the Student-Mentor loop
/// 8. Cleanup
#[allow(clippy::too_many_lines)]
async fn run_smile_loop(args: Args) -> anyhow::Result<()> {
    // Load configuration
    let mut config = load_config(args.config.as_deref())?;

    // Apply CLI argument overrides
    if let Some(ref tutorial) = args.tutorial {
        config.tutorial.clone_from(tutorial);
    }
    if let Some(ref output_dir) = args.output_dir {
        config.output_dir.clone_from(output_dir);
    }

    // Re-validate after overrides
    config.validate()?;

    print_config(&config);

    // Load tutorial file with images
    tracing::info!(tutorial = %config.tutorial, "Loading tutorial");
    let tutorial = Tutorial::load_with_images(&config.tutorial)?;
    print_tutorial_info(&tutorial);

    // Step 1: Check Docker availability
    println!();
    println!("Checking Docker availability...");
    let container_manager = check_docker_available()?;
    container_manager.health_check().await.map_err(|e| {
        anyhow::anyhow!(
            "Docker health check failed: {e}\n\nSuggestion: Make sure Docker is running and accessible"
        )
    })?;
    println!("Docker is available and healthy");

    // Step 2: Acquire state lock
    let state_path = PathBuf::from(&config.state_file);
    println!();
    println!("Acquiring state lock...");
    let _lock = acquire_state_lock(&state_path).await?;
    println!("State lock acquired");

    // Step 3: Load or create state
    let loop_state = load_or_create_state(&state_path, args.resume).await?;
    let loop_state = Arc::new(Mutex::new(loop_state));

    // Step 4: Setup container
    let tutorial_dir = tutorial
        .path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let tutorial_dir = tutorial_dir.canonicalize().map_err(|e| {
        anyhow::anyhow!(
            "Failed to resolve tutorial directory: {e}\n\nPath: {}",
            tutorial_dir.display()
        )
    })?;

    // Create work directory
    let work_dir = PathBuf::from(&config.output_dir).join(".smile/work");
    tokio::fs::create_dir_all(&work_dir).await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to create work directory: {e}\n\nPath: {}",
            work_dir.display()
        )
    })?;
    let work_dir = work_dir.canonicalize().map_err(|e| {
        anyhow::anyhow!(
            "Failed to resolve work directory: {e}\n\nPath: {}",
            work_dir.display()
        )
    })?;

    let session_id = generate_session_id();
    let container_name = format!("smile-{session_id}");

    println!();
    println!("Setting up container: {container_name}");
    tracing::info!(
        container_name = %container_name,
        tutorial_dir = %tutorial_dir.display(),
        work_dir = %work_dir.display(),
        "Creating container"
    );

    let container_options = CreateContainerOptions::new(&container_name, &config.container_image)
        .with_mount(Mount::read_only(&tutorial_dir, "/workspace/tutorial"))
        .with_mount(Mount::new(&work_dir, "/workspace/work"))
        .with_env("SMILE_SESSION", &session_id)
        .with_env("SMILE_API_HOST", "host.docker.internal")
        .with_env("SMILE_API_PORT", args.port.to_string())
        .with_cmd(vec!["sleep", "infinity"]);

    let mut container = container_manager
        .create_container(container_options)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to create container: {e}\n\nSuggestion: Make sure the image '{}' exists",
                config.container_image
            )
        })?;
    println!("Container created: {}", container.id);

    // Start the container
    container_manager
        .start_container(&mut container)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start container: {e}"))?;
    println!("Container started");

    // Step 5: Start HTTP server in background
    let addr: SocketAddr = ([127, 0, 0, 1], args.port).into();
    println!();
    println!("Starting HTTP API server on {addr}...");

    let app_state = AppState {
        config: config.clone(),
        loop_state: Arc::clone(&loop_state),
        broadcaster: EventBroadcaster::default(),
    };
    let router = create_router(app_state);

    let listener = TcpListener::bind(addr).await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to bind to {addr}: {e}\n\nSuggestion: Try a different port with --port"
        )
    })?;

    // Spawn the server in the background
    let server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!(error = %e, "HTTP server error");
        }
    });

    println!("HTTP API server running on http://{addr}");

    // Step 6: Run the main loop
    println!();
    println!("Starting SMILE Loop...");
    println!("Press Ctrl+C to stop");
    println!();

    let loop_result = run_main_loop(
        &loop_state,
        &state_path,
        &config,
        &container_manager,
        &mut container,
    )
    .await;

    // Step 7: Cleanup
    println!();
    println!("Cleaning up...");

    // Stop the container
    if container.is_running() {
        tracing::info!(container_id = %container.id, "Stopping container");
        if let Err(e) = container_manager
            .stop_container(&mut container, Some(10))
            .await
        {
            tracing::warn!(error = %e, "Failed to stop container");
        }
    }

    // Remove the container
    tracing::info!(container_id = %container.id, "Removing container");
    if let Err(e) = container_manager
        .remove_container(&mut container, true)
        .await
    {
        tracing::warn!(error = %e, "Failed to remove container");
    }
    println!("Container removed");

    // Cancel the server
    server_handle.abort();

    // Save final state and print summary
    let final_state = {
        let state = loop_state.lock().await;
        state.save(&state_path).await?;
        println!("Final state saved to {}", state_path.display());
        state.clone()
    };

    println!();
    print_summary(&final_state, &config);

    // Generate reports
    let report_dir = PathBuf::from(&config.output_dir);
    generate_reports(&final_state, &tutorial, &report_dir)?;

    loop_result
}

/// Runs the main Student-Mentor loop.
///
/// This function polls the shared state waiting for callbacks from the
/// agent wrappers via the HTTP API.
async fn run_main_loop(
    loop_state: &Arc<Mutex<LoopState>>,
    state_path: &Path,
    config: &Config,
    _container_manager: &ContainerManager,
    _container: &mut smile_container::Container,
) -> anyhow::Result<()> {
    // Initialize the loop if starting fresh
    {
        let mut state = loop_state.lock().await;
        if state.status == LoopStatus::Starting {
            state.start()?;
            state.start_waiting_for_student()?;
            state.save(state_path).await?;
            tracing::info!(
                iteration = state.iteration,
                "Loop started, waiting for student"
            );
            println!("Iteration 1: Waiting for student agent...");
        }
    }

    // Main polling loop with graceful shutdown on Ctrl+C
    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);
    let mut last_status = LoopStatus::Starting;
    let mut last_iteration = 0u32;

    loop {
        // Use select to handle both polling and Ctrl+C
        tokio::select! {
            Ok(()) = tokio::signal::ctrl_c() => {
                tracing::info!("Received Ctrl+C, shutting down");
                let mut state = loop_state.lock().await;
                if !state.is_terminal() {
                    let _ = state.error("User interrupted".to_string());
                }
                drop(state);
                break;
            }
            () = sleep(poll_interval) => {
                // Check current state
                let (status, iteration, is_terminal) = {
                    let mut state = loop_state.lock().await;

                    // Check termination conditions
                    if let Some(terminal_status) = state.check_termination(config.max_iterations, config.timeout)
                    {
                        tracing::info!(status = %terminal_status, "Termination condition met");
                        return Ok(());
                    }

                    (state.status, state.iteration, state.is_terminal())
                };

                // Print status changes
                if status != last_status {
                    print_status_change(status, iteration);
                    last_status = status;
                }

                // Print iteration changes
                if iteration != last_iteration && iteration > 0 {
                    println!("Iteration {iteration}: {status}");
                    last_iteration = iteration;
                }

                // Check if we've reached a terminal state
                if is_terminal {
                    tracing::info!(status = %status, "Loop reached terminal state");
                    break;
                }

                // Save state periodically (every poll)
                {
                    let state = loop_state.lock().await;
                    if let Err(e) = state.save(state_path).await {
                        tracing::warn!(error = %e, "Failed to save state");
                    }
                }
            }
        }
    }

    Ok(())
}

/// Loads configuration from the specified path or default location.
fn load_config(config_path: Option<&str>) -> anyhow::Result<Config> {
    match config_path {
        Some(path_str) => {
            let path = Path::new(path_str);
            if !path.exists() {
                anyhow::bail!(
                    "Config file not found: '{}'\n\nSuggestion: Check the path or remove the --config flag to use defaults",
                    path.display()
                );
            }
            Config::load_from_file(path).map_err(|e| anyhow::anyhow!("{e}"))
        }
        None => Config::load().map_err(|e| anyhow::anyhow!("{e}")),
    }
}

/// Checks if Docker is available by creating a `ContainerManager`.
fn check_docker_available() -> anyhow::Result<ContainerManager> {
    ContainerManager::new().map_err(|e| {
        anyhow::anyhow!(
            "Docker is not available: {e}\n\nSuggestion: Make sure Docker is installed and running"
        )
    })
}

/// Acquires an exclusive lock on the state file.
async fn acquire_state_lock(state_path: &Path) -> anyhow::Result<StateLock> {
    LoopState::acquire_lock(state_path).await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to acquire state lock: {e}\n\nSuggestion: Check if another SMILE loop is running"
        )
    })
}

/// Loads existing state or creates a new one.
async fn load_or_create_state(state_path: &Path, resume: bool) -> anyhow::Result<LoopState> {
    let existing_state = LoopState::load(state_path).await?;

    match existing_state {
        Some(state) if resume => {
            println!(
                "Resuming from existing state (iteration {})",
                state.iteration
            );
            tracing::info!(
                iteration = state.iteration,
                status = %state.status,
                "Resuming from existing state"
            );
            Ok(state)
        }
        Some(state) if !state.is_terminal() => {
            // There's an active state but --resume wasn't specified
            anyhow::bail!(
                "Found active state file at '{}' (iteration {})\n\nSuggestion: Use --resume to continue or delete the state file to start fresh",
                state_path.display(),
                state.iteration
            );
        }
        Some(_) | None => {
            // No state or terminal state - start fresh
            println!("Starting fresh SMILE loop");
            tracing::info!("Creating new loop state");
            Ok(LoopState::new())
        }
    }
}

/// Generates a unique session ID for container naming.
fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    format!("{timestamp:x}")
}

/// Prints the loaded configuration.
fn print_config(config: &Config) {
    println!("Configuration loaded:");
    println!("  Tutorial: {}", config.tutorial);
    println!("  Output directory: {}", config.output_dir);
    println!("  LLM provider: {:?}", config.llm_provider);
    println!("  Max iterations: {}", config.max_iterations);
    println!("  Timeout: {}s", config.timeout);
    println!("  Container image: {}", config.container_image);
}

/// Prints tutorial information.
fn print_tutorial_info(tutorial: &Tutorial) {
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
}

/// Prints status change messages.
fn print_status_change(status: LoopStatus, iteration: u32) {
    match status {
        LoopStatus::WaitingForStudent => {
            tracing::debug!(iteration, "Waiting for student callback");
        }
        LoopStatus::RunningMentor => {
            println!("  Student needs help, invoking mentor...");
            tracing::info!(iteration, "Student escalated to mentor");
        }
        LoopStatus::WaitingForMentor => {
            tracing::debug!(iteration, "Waiting for mentor callback");
        }
        LoopStatus::RunningStudent => {
            tracing::debug!(iteration, "Running student agent");
        }
        LoopStatus::Completed => {
            println!("  Tutorial completed successfully!");
        }
        LoopStatus::MaxIterations => {
            println!("  Maximum iterations reached");
        }
        LoopStatus::Timeout => {
            println!("  Timeout exceeded");
        }
        LoopStatus::Blocker => {
            println!("  Student encountered a blocker");
        }
        LoopStatus::Error => {
            println!("  Error occurred");
        }
        LoopStatus::Starting => {}
    }
}

/// Prints a summary of the loop execution.
fn print_summary(state: &LoopState, config: &Config) {
    println!("=== SMILE Loop Summary ===");
    println!("Status: {}", state.status);
    println!("Iterations: {}", state.iteration);
    println!("Mentor consultations: {}", state.mentor_notes.len());

    if let Some(reason) = state.termination_summary(config.max_iterations, config.timeout) {
        println!("Termination: {reason}");
    }

    let elapsed = state.elapsed();
    println!(
        "Duration: {}m {}s",
        elapsed.num_minutes(),
        elapsed.num_seconds() % 60
    );
}

/// Generates reports from the final loop state.
///
/// Creates both Markdown and JSON reports in the output directory.
fn generate_reports(
    state: &LoopState,
    tutorial: &Tutorial,
    output_dir: &Path,
) -> anyhow::Result<()> {
    println!();
    println!("Generating reports...");

    // Create ReportInput from state
    let tutorial_name = tutorial.path.file_stem().map_or_else(
        || "Unknown".to_string(),
        |s| s.to_string_lossy().to_string(),
    );
    let tutorial_path = tutorial.path.display().to_string();
    let input = create_report_input(state, &tutorial_name, &tutorial_path);

    // Generate report
    let generator = ReportGenerator::new(input);
    let report = generator.generate();

    // Ensure output directory exists
    std::fs::create_dir_all(output_dir)?;

    // Write Markdown report
    let md_generator = MarkdownGenerator::new(&report);
    let markdown = md_generator.generate();
    let md_path = output_dir.join("smile-report.md");
    std::fs::write(&md_path, markdown)?;
    println!("  Markdown report: {}", md_path.display());

    // Write JSON report
    let json_path = output_dir.join("smile-report.json");
    let json_generator = JsonGenerator::new(&report);
    json_generator.write_to_file(&json_path, true)?;
    println!("  JSON report: {}", json_path.display());

    // Print gap summary
    let counts = report.gap_counts();
    println!();
    if counts.total() > 0 {
        println!(
            "Gaps found: {} ({} critical, {} major, {} minor)",
            counts.total(),
            counts.critical,
            counts.major,
            counts.minor
        );
    } else {
        println!("No documentation gaps found!");
    }

    Ok(())
}

/// Creates a `ReportInput` from the loop state.
fn create_report_input(state: &LoopState, tutorial_name: &str, tutorial_path: &str) -> ReportInput {
    ReportInput {
        tutorial_name: tutorial_name.to_string(),
        tutorial_path: tutorial_path.to_string(),
        status: convert_status(state.status),
        iterations: state.iteration,
        started_at: state.started_at,
        ended_at: state.updated_at,
        history: state.history.iter().map(convert_iteration).collect(),
        mentor_notes: state.mentor_notes.iter().map(convert_mentor_note).collect(),
    }
}

/// Converts `LoopStatus` to `ReportStatus`.
const fn convert_status(status: LoopStatus) -> ReportStatus {
    match status {
        LoopStatus::Completed => ReportStatus::Completed,
        LoopStatus::MaxIterations => ReportStatus::MaxIterations,
        LoopStatus::Blocker => ReportStatus::Blocker,
        LoopStatus::Timeout => ReportStatus::Timeout,
        LoopStatus::Error => ReportStatus::Error,
        // Non-terminal states shouldn't appear in final report
        LoopStatus::Starting => ReportStatus::Starting,
        LoopStatus::RunningStudent => ReportStatus::RunningStudent,
        LoopStatus::WaitingForStudent => ReportStatus::WaitingForStudent,
        LoopStatus::RunningMentor => ReportStatus::RunningMentor,
        LoopStatus::WaitingForMentor => ReportStatus::WaitingForMentor,
    }
}

/// Converts `StudentStatus` to `StudentStatusInput`.
const fn convert_student_status(status: StudentStatus) -> StudentStatusInput {
    match status {
        StudentStatus::Completed => StudentStatusInput::Completed,
        StudentStatus::AskMentor => StudentStatusInput::AskMentor,
        StudentStatus::CannotComplete => StudentStatusInput::CannotComplete,
    }
}

/// Converts an `IterationRecord` to `IterationInput`.
fn convert_iteration(record: &IterationRecord) -> IterationInput {
    IterationInput {
        iteration: record.iteration,
        student_status: convert_student_status(record.student_output.status),
        current_step: record.student_output.current_step.clone(),
        problem: record.student_output.problem.clone(),
        question_for_mentor: record.student_output.question_for_mentor.clone(),
        reason: record.student_output.reason.clone(),
        summary: record.student_output.summary.clone(),
        files_created: record.student_output.files_created.clone(),
        commands_run: record.student_output.commands_run.clone(),
        started_at: record.started_at,
        ended_at: record.ended_at,
    }
}

/// Converts a `MentorNote` to `MentorNoteInput`.
fn convert_mentor_note(note: &MentorNote) -> MentorNoteInput {
    MentorNoteInput {
        iteration: note.iteration,
        question: note.question.clone(),
        answer: note.answer.clone(),
        timestamp: note.timestamp,
    }
}
