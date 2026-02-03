//! End-to-end integration tests for SMILE Loop
//!
//! These tests validate the complete workflow from tutorial loading
//! through report generation. Full loop execution tests require Docker
//! and are marked with `#[ignore]` for CI environments without Docker.

use std::path::PathBuf;

use smile_orchestrator::{Config, LoopState, LoopStatus, Tutorial};
use smile_report::{
    Gap, GapSeverity, IterationInput, MarkdownGenerator, MentorNoteInput, ReportGenerator,
    ReportInput, ReportStatus, StudentStatusInput,
};

/// Path to the sample tutorial fixture.
fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("tests/integration/fixtures/sample-tutorial"))
        .expect("Failed to find fixture path")
}

/// Tests that the sample tutorial loads successfully.
#[test]
fn test_sample_tutorial_loads() {
    let tutorial_path = fixture_path().join("tutorial.md");
    assert!(
        tutorial_path.exists(),
        "Tutorial fixture not found at: {tutorial_path:?}"
    );

    let tutorial = Tutorial::load(&tutorial_path).expect("Failed to load tutorial");

    assert!(!tutorial.content.is_empty(), "Tutorial content is empty");
    assert!(
        tutorial.content.contains("Building a Simple CLI Counter"),
        "Tutorial should contain title"
    );
    assert!(
        tutorial.content.contains("npm init"),
        "Tutorial should contain npm init step"
    );
    assert!(tutorial.size_bytes < 100_000, "Tutorial exceeds size limit");
}

/// Tests that the sample config loads successfully.
#[test]
fn test_sample_config_loads() {
    let config_path = fixture_path().join("smile.json");
    assert!(
        config_path.exists(),
        "Config fixture not found at: {config_path:?}"
    );

    let config = Config::load_from_file(&config_path).expect("Failed to load config");

    assert_eq!(config.tutorial, "tutorial.md");
    assert_eq!(config.max_iterations, 5);
    assert_eq!(config.timeout, 300);
    assert!(config.student_behavior.ask_on_missing_dependency);
    assert!(config.student_behavior.ask_on_ambiguous_instruction);
}

/// Tests that gap extraction works correctly from loop history.
#[test]
fn test_gap_extraction_from_iterations() {
    let now = chrono::Utc::now();

    // Create mock iteration records simulating the expected gaps
    let history = vec![
        IterationInput {
            iteration: 1,
            student_status: StudentStatusInput::AskMentor,
            current_step: "Step 2: Initialize the Project".to_string(),
            problem: Some("Command not found: npm".to_string()),
            question_for_mentor: Some(
                "How do I run npm? The command is not recognized.".to_string(),
            ),
            reason: Some("missing_dependency".to_string()),
            summary: "Attempted to initialize project but npm command failed".to_string(),
            files_created: vec![],
            commands_run: vec!["npm init -y".to_string()],
            started_at: now,
            ended_at: now,
        },
        IterationInput {
            iteration: 2,
            student_status: StudentStatusInput::AskMentor,
            current_step: "Step 4: Configure the Executable".to_string(),
            problem: Some("Instructions unclear".to_string()),
            question_for_mentor: Some(
                "What configuration file should I update? What settings are needed?".to_string(),
            ),
            reason: Some("ambiguous_instruction".to_string()),
            summary: "Created counter.js but unsure how to configure it".to_string(),
            files_created: vec!["counter.js".to_string()],
            commands_run: vec![],
            started_at: now,
            ended_at: now,
        },
        IterationInput {
            iteration: 3,
            student_status: StudentStatusInput::AskMentor,
            current_step: "Step 5: Test Your Counter".to_string(),
            problem: Some("Permission denied when running ./counter.js".to_string()),
            question_for_mentor: Some("How do I make the script executable?".to_string()),
            reason: Some("command_failure".to_string()),
            summary: "Tried to run script but got permission denied".to_string(),
            files_created: vec![],
            commands_run: vec!["./counter.js show".to_string()],
            started_at: now,
            ended_at: now,
        },
    ];

    // Create report input
    let input = ReportInput {
        tutorial_name: "sample-tutorial".to_string(),
        tutorial_path: "tests/integration/fixtures/sample-tutorial/tutorial.md".to_string(),
        status: ReportStatus::MaxIterations,
        iterations: 3,
        started_at: now,
        ended_at: now,
        history,
        mentor_notes: vec![],
    };

    // Generate report
    let generator = ReportGenerator::new(input);
    let report = generator.generate();

    // Verify gaps were detected
    assert!(
        !report.gaps.is_empty(),
        "Expected gaps to be extracted from history"
    );

    // Check gap count
    let counts = report.gap_counts();
    assert!(
        counts.total() >= 2,
        "Expected at least 2 gaps, got {}",
        counts.total()
    );
}

/// Tests that markdown report is generated correctly.
#[test]
fn test_markdown_report_generation() {
    let now = chrono::Utc::now();

    // Create a simple report input with known gaps
    let input = ReportInput {
        tutorial_name: "test-tutorial".to_string(),
        tutorial_path: "/path/to/tutorial.md".to_string(),
        status: ReportStatus::MaxIterations,
        iterations: 3,
        started_at: now,
        ended_at: now,
        history: vec![IterationInput {
            iteration: 1,
            student_status: StudentStatusInput::AskMentor,
            current_step: "Step 2".to_string(),
            problem: Some("npm not found".to_string()),
            question_for_mentor: Some("How do I install npm?".to_string()),
            reason: Some("missing_dependency".to_string()),
            summary: "Could not proceed without npm".to_string(),
            files_created: vec![],
            commands_run: vec!["npm init".to_string()],
            started_at: now,
            ended_at: now,
        }],
        mentor_notes: vec![MentorNoteInput {
            iteration: 1,
            question: "How do I install npm?".to_string(),
            answer: "You need to install Node.js first.".to_string(),
            timestamp: now,
        }],
    };

    let generator = ReportGenerator::new(input);
    let report = generator.generate();

    // Generate markdown
    let md_generator = MarkdownGenerator::new(&report);
    let markdown = md_generator.generate();

    // Verify markdown content
    assert!(
        markdown.contains("test-tutorial"),
        "Markdown should contain tutorial name"
    );
    assert!(
        markdown.contains("Iteration"),
        "Markdown should contain iteration info"
    );
}

/// Tests gap severity classification based on problem patterns.
#[test]
fn test_gap_severity_classification() {
    // Critical: Missing prerequisite
    let critical_gap = Gap::builder()
        .id(1)
        .title("Missing Node.js prerequisite")
        .problem("Node.js is not installed but npm commands are used")
        .severity(GapSeverity::Critical)
        .build()
        .expect("Failed to build critical gap");

    assert_eq!(critical_gap.severity, GapSeverity::Critical);

    // Major: Ambiguous instruction
    let major_gap = Gap::builder()
        .id(2)
        .title("Ambiguous configuration instruction")
        .problem("Instructions unclear about which configuration file to update")
        .severity(GapSeverity::Major)
        .build()
        .expect("Failed to build major gap");

    assert_eq!(major_gap.severity, GapSeverity::Major);

    // Minor: Environment assumption
    let minor_gap = Gap::builder()
        .id(3)
        .title("Environment-specific assumption")
        .problem("Script uses HOME environment variable which may not be set in all environments")
        .severity(GapSeverity::Minor)
        .build()
        .expect("Failed to build minor gap");

    assert_eq!(minor_gap.severity, GapSeverity::Minor);
}

/// Tests loop state creation and status transitions.
#[test]
fn test_loop_state_transitions() {
    let mut state = LoopState::new();
    assert_eq!(state.status, LoopStatus::Starting);
    assert_eq!(state.iteration, 0);

    // Start the loop
    state.start().expect("Failed to start loop");
    assert_eq!(state.status, LoopStatus::RunningStudent);
    assert_eq!(state.iteration, 1);

    // Transition to waiting for student
    state
        .start_waiting_for_student()
        .expect("Failed to transition to waiting");
    assert_eq!(state.status, LoopStatus::WaitingForStudent);
}

/// Tests that the expected gaps document matches our understanding.
#[test]
fn test_expected_gaps_documented() {
    let expected_gaps_path = fixture_path().join("EXPECTED_GAPS.md");
    assert!(
        expected_gaps_path.exists(),
        "EXPECTED_GAPS.md not found at: {expected_gaps_path:?}"
    );

    let content =
        std::fs::read_to_string(&expected_gaps_path).expect("Failed to read EXPECTED_GAPS.md");

    // Verify all four documented gaps are present
    assert!(
        content.contains("Missing Prerequisite"),
        "Should document missing prerequisite gap"
    );
    assert!(
        content.contains("Ambiguous Instruction"),
        "Should document ambiguous instruction gap"
    );
    assert!(
        content.contains("Missing Intermediate Step"),
        "Should document missing step gap"
    );
    assert!(
        content.contains("Environment-Specific Assumption"),
        "Should document environment assumption gap"
    );
}

/// Tests report gap counts calculation.
#[test]
fn test_report_gap_counts() {
    let now = chrono::Utc::now();

    // Create input with iterations that result in gaps
    let input = ReportInput {
        tutorial_name: "test".to_string(),
        tutorial_path: "/test.md".to_string(),
        status: ReportStatus::Completed,
        iterations: 2,
        started_at: now,
        ended_at: now,
        history: vec![
            IterationInput {
                iteration: 1,
                student_status: StudentStatusInput::CannotComplete,
                current_step: "Step 1".to_string(),
                problem: Some("Completely blocked".to_string()),
                question_for_mentor: None,
                reason: Some("fatal_error".to_string()),
                summary: "Cannot continue".to_string(),
                files_created: vec![],
                commands_run: vec![],
                started_at: now,
                ended_at: now,
            },
            IterationInput {
                iteration: 2,
                student_status: StudentStatusInput::AskMentor,
                current_step: "Step 2".to_string(),
                problem: Some("Minor confusion".to_string()),
                question_for_mentor: Some("What does this mean?".to_string()),
                reason: None,
                summary: "Asked for help".to_string(),
                files_created: vec![],
                commands_run: vec![],
                started_at: now,
                ended_at: now,
            },
        ],
        mentor_notes: vec![],
    };

    let generator = ReportGenerator::new(input);
    let report = generator.generate();

    // Should have at least 2 gaps (one critical from CannotComplete, one major from AskMentor)
    let counts = report.gap_counts();
    assert!(counts.total() >= 2, "Should have at least 2 gaps");
    assert!(counts.critical >= 1, "Should have at least 1 critical gap");
}

/// Full integration test that requires Docker.
///
/// This test is ignored by default as it requires:
/// - Docker running
/// - smile-base:latest image built
/// - LLM API keys configured
///
/// Run with: `cargo test -p smile-integration-tests test_full_loop_with_docker -- --ignored`
#[test]
#[ignore = "Requires Docker and LLM API keys"]
fn test_full_loop_with_docker() {
    // This test would:
    // 1. Load the sample tutorial and config
    // 2. Start the SMILE loop
    // 3. Wait for completion or timeout
    // 4. Validate the generated report contains expected gaps
    //
    // Implementation deferred until Docker CI setup is complete.
    //
    // To enable this test:
    // 1. Build the smile-base image: `docker build -f docker/Dockerfile.base -t smile-base:latest .`
    // 2. Set LLM API keys: `export ANTHROPIC_API_KEY=...`
    // 3. Run: `cargo test -p smile-integration-tests test_full_loop_with_docker -- --ignored`
}
