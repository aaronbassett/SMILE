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

// ============================================================================
// Python FastAPI Tutorial Tests
// ============================================================================

/// Path to the Python FastAPI tutorial fixture.
fn fastapi_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("tests/integration/fixtures/python-fastapi-tutorial"))
        .expect("Failed to find FastAPI fixture path")
}

/// Tests that the Python FastAPI tutorial loads successfully.
#[test]
fn test_fastapi_tutorial_loads() {
    let tutorial_path = fastapi_fixture_path().join("tutorial.md");
    assert!(
        tutorial_path.exists(),
        "FastAPI tutorial fixture not found at: {tutorial_path:?}"
    );

    let tutorial = Tutorial::load(&tutorial_path).expect("Failed to load FastAPI tutorial");

    assert!(!tutorial.content.is_empty(), "Tutorial content is empty");
    assert!(
        tutorial
            .content
            .contains("Building a FastAPI Hello World Server"),
        "Tutorial should contain title"
    );
    assert!(
        tutorial.content.contains("pip install fastapi"),
        "Tutorial should contain fastapi installation step"
    );
    assert!(
        tutorial.content.contains("uvicorn main:app"),
        "Tutorial should contain uvicorn run command"
    );
}

/// Tests that the Python FastAPI config loads successfully.
#[test]
fn test_fastapi_config_loads() {
    let config_path = fastapi_fixture_path().join("smile.json");
    assert!(
        config_path.exists(),
        "FastAPI config fixture not found at: {config_path:?}"
    );

    let config = Config::load_from_file(&config_path).expect("Failed to load FastAPI config");

    assert_eq!(config.tutorial, "tutorial.md");
    assert_eq!(config.max_iterations, 6);
    assert!(config.student_behavior.ask_on_command_failure);
}

/// Tests that expected gaps document exists for FastAPI tutorial.
#[test]
fn test_fastapi_expected_gaps_documented() {
    let expected_gaps_path = fastapi_fixture_path().join("EXPECTED_GAPS.md");
    assert!(
        expected_gaps_path.exists(),
        "FastAPI EXPECTED_GAPS.md not found at: {expected_gaps_path:?}"
    );

    let content =
        std::fs::read_to_string(&expected_gaps_path).expect("Failed to read EXPECTED_GAPS.md");

    // Verify documented gaps
    assert!(
        content.contains("Python Version"),
        "Should document Python version gap"
    );
    assert!(
        content.contains("Platform-Specific"),
        "Should document platform-specific activation command gap"
    );
    assert!(
        content.contains("Port Conflict"),
        "Should document port conflict gap"
    );
}

/// Tests gap extraction for Python FastAPI tutorial scenario.
#[test]
fn test_fastapi_gap_extraction() {
    let now = chrono::Utc::now();

    // Create mock iterations simulating Python version and port conflict issues
    let history = vec![
        IterationInput {
            iteration: 1,
            student_status: StudentStatusInput::AskMentor,
            current_step: "Step 2: Create Virtual Environment".to_string(),
            problem: Some("Command 'python' not found".to_string()),
            question_for_mentor: Some(
                "The tutorial says python but the command is not found. Should I use python3?"
                    .to_string(),
            ),
            reason: Some("missing_dependency".to_string()),
            summary: "Cannot create virtual environment".to_string(),
            files_created: vec!["fastapi-hello/".to_string()],
            commands_run: vec!["python -m venv venv".to_string()],
            started_at: now,
            ended_at: now,
        },
        IterationInput {
            iteration: 2,
            student_status: StudentStatusInput::AskMentor,
            current_step: "Step 5: Run the Server".to_string(),
            problem: Some("Address already in use - port 8000 is taken".to_string()),
            question_for_mentor: Some(
                "The server fails because port 8000 is in use. How do I fix this?".to_string(),
            ),
            reason: Some("command_failure".to_string()),
            summary: "Server cannot start due to port conflict".to_string(),
            files_created: vec!["main.py".to_string()],
            commands_run: vec!["uvicorn main:app --reload".to_string()],
            started_at: now,
            ended_at: now,
        },
    ];

    let input = ReportInput {
        tutorial_name: "python-fastapi-tutorial".to_string(),
        tutorial_path: "tests/integration/fixtures/python-fastapi-tutorial/tutorial.md".to_string(),
        status: ReportStatus::MaxIterations,
        iterations: 2,
        started_at: now,
        ended_at: now,
        history,
        mentor_notes: vec![],
    };

    let generator = ReportGenerator::new(input);
    let report = generator.generate();

    assert!(
        !report.gaps.is_empty(),
        "Expected gaps to be extracted from FastAPI scenario"
    );
    assert!(
        report.gap_counts().total() >= 2,
        "Expected at least 2 gaps from FastAPI scenario"
    );
}

// ============================================================================
// Docker Setup Tutorial Tests
// ============================================================================

/// Path to the Docker setup tutorial fixture.
fn docker_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("tests/integration/fixtures/docker-setup-tutorial"))
        .expect("Failed to find Docker fixture path")
}

/// Tests that the Docker setup tutorial loads successfully.
#[test]
fn test_docker_tutorial_loads() {
    let tutorial_path = docker_fixture_path().join("tutorial.md");
    assert!(
        tutorial_path.exists(),
        "Docker tutorial fixture not found at: {tutorial_path:?}"
    );

    let tutorial = Tutorial::load(&tutorial_path).expect("Failed to load Docker tutorial");

    assert!(!tutorial.content.is_empty(), "Tutorial content is empty");
    assert!(
        tutorial
            .content
            .contains("Getting Started with Docker Containers"),
        "Tutorial should contain title"
    );
    assert!(
        tutorial.content.contains("docker pull hello-world"),
        "Tutorial should contain docker pull step"
    );
    assert!(
        tutorial.content.contains("docker run hello-world"),
        "Tutorial should contain docker run command"
    );
}

/// Tests that the Docker setup config loads successfully.
#[test]
fn test_docker_config_loads() {
    let config_path = docker_fixture_path().join("smile.json");
    assert!(
        config_path.exists(),
        "Docker config fixture not found at: {config_path:?}"
    );

    let config = Config::load_from_file(&config_path).expect("Failed to load Docker config");

    assert_eq!(config.tutorial, "tutorial.md");
    assert_eq!(config.max_iterations, 5);
}

/// Tests that expected gaps document exists for Docker tutorial.
#[test]
fn test_docker_expected_gaps_documented() {
    let expected_gaps_path = docker_fixture_path().join("EXPECTED_GAPS.md");
    assert!(
        expected_gaps_path.exists(),
        "Docker EXPECTED_GAPS.md not found at: {expected_gaps_path:?}"
    );

    let content =
        std::fs::read_to_string(&expected_gaps_path).expect("Failed to read EXPECTED_GAPS.md");

    // Verify documented gaps
    assert!(
        content.contains("Docker Not Installed"),
        "Should document Docker installation gap"
    );
    assert!(
        content.contains("Daemon Not Running"),
        "Should document daemon not running gap"
    );
    assert!(
        content.contains("Linux Permission"),
        "Should document Linux permission requirements"
    );
}

/// Tests gap extraction for Docker setup tutorial scenario.
#[test]
fn test_docker_gap_extraction() {
    let now = chrono::Utc::now();

    // Create mock iterations simulating Docker installation and daemon issues
    // The first issue (Docker not installed) is a complete blocker -> CannotComplete
    let history = vec![
        IterationInput {
            iteration: 1,
            student_status: StudentStatusInput::CannotComplete,
            current_step: "Step 1: Verify Docker Installation".to_string(),
            problem: Some("Command 'docker' not found".to_string()),
            question_for_mentor: None,
            reason: Some("Docker is not installed - cannot proceed without it".to_string()),
            summary: "Docker is not installed".to_string(),
            files_created: vec![],
            commands_run: vec!["docker --version".to_string()],
            started_at: now,
            ended_at: now,
        },
        IterationInput {
            iteration: 2,
            student_status: StudentStatusInput::AskMentor,
            current_step: "Step 2: Pull Your First Image".to_string(),
            problem: Some("Cannot connect to the Docker daemon".to_string()),
            question_for_mentor: Some(
                "Docker is installed but daemon is not running. How do I start it?".to_string(),
            ),
            reason: Some("command_failure".to_string()),
            summary: "Docker daemon not running".to_string(),
            files_created: vec![],
            commands_run: vec!["docker pull hello-world".to_string()],
            started_at: now,
            ended_at: now,
        },
        IterationInput {
            iteration: 3,
            student_status: StudentStatusInput::AskMentor,
            current_step: "Step 3: Run the Container".to_string(),
            problem: Some("Permission denied when connecting to Docker socket".to_string()),
            question_for_mentor: Some(
                "I get permission denied on docker commands. Do I need sudo?".to_string(),
            ),
            reason: Some("permission_error".to_string()),
            summary: "Permission denied for Docker commands".to_string(),
            files_created: vec![],
            commands_run: vec!["docker run hello-world".to_string()],
            started_at: now,
            ended_at: now,
        },
    ];

    let input = ReportInput {
        tutorial_name: "docker-setup-tutorial".to_string(),
        tutorial_path: "tests/integration/fixtures/docker-setup-tutorial/tutorial.md".to_string(),
        status: ReportStatus::MaxIterations,
        iterations: 3,
        started_at: now,
        ended_at: now,
        history,
        mentor_notes: vec![],
    };

    let generator = ReportGenerator::new(input);
    let report = generator.generate();

    assert!(
        !report.gaps.is_empty(),
        "Expected gaps to be extracted from Docker scenario"
    );

    let counts = report.gap_counts();
    assert!(
        counts.total() >= 3,
        "Expected at least 3 gaps from Docker scenario, got {}",
        counts.total()
    );
    assert!(
        counts.critical >= 1,
        "Expected at least 1 critical gap (Docker not installed)"
    );
}

// ============================================================================
// Mock Scenario File Tests
// ============================================================================

/// Tests that mock scenario files are valid JSON.
#[test]
fn test_mock_scenarios_valid_json() {
    let mock_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("tests/integration/fixtures/mock-cli/scenarios"))
        .expect("Failed to find mock scenarios directory");

    let scenarios = [
        "missing_npm.json",
        "mentor_responses.json",
        "python_fastapi.json",
        "docker_setup.json",
        "python_fastapi_mentor.json",
        "docker_setup_mentor.json",
    ];

    for scenario in scenarios {
        let path = mock_dir.join(scenario);
        assert!(path.exists(), "Mock scenario not found: {scenario}");

        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read {scenario}"));
        let _: serde_json::Value = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Invalid JSON in {scenario}: {e}"));
    }
}

/// Tests mock scenario response structure.
#[test]
fn test_mock_scenario_response_structure() {
    let mock_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("tests/integration/fixtures/mock-cli/scenarios"))
        .expect("Failed to find mock scenarios directory");

    // Test student scenarios have required fields
    let student_scenarios = [
        "missing_npm.json",
        "python_fastapi.json",
        "docker_setup.json",
    ];

    for scenario in student_scenarios {
        let path = mock_dir.join(scenario);
        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read {scenario}"));
        let json: serde_json::Value =
            serde_json::from_str(&content).expect("Invalid JSON in scenario");

        let responses = json["responses"]
            .as_array()
            .expect("Missing responses array");
        assert!(!responses.is_empty(), "Responses should not be empty");

        for (i, response) in responses.iter().enumerate() {
            assert!(
                response.get("status").is_some(),
                "{scenario} response {i} missing status"
            );
            assert!(
                response.get("currentStep").is_some(),
                "{scenario} response {i} missing currentStep"
            );
            assert!(
                response.get("summary").is_some(),
                "{scenario} response {i} missing summary"
            );
        }
    }
}

// ============================================================================
// Report Quality Validation Tests
// ============================================================================

/// Tests that the generated markdown report for the sample tutorial is readable
/// and contains all expected sections.
#[test]
fn test_sample_tutorial_report_quality() {
    let now = chrono::Utc::now();

    // Create a realistic report input based on the sample tutorial gaps
    let input = ReportInput {
        tutorial_name: "sample-tutorial".to_string(),
        tutorial_path: "tests/integration/fixtures/sample-tutorial/tutorial.md".to_string(),
        status: ReportStatus::Completed,
        iterations: 4,
        started_at: now - chrono::Duration::minutes(5),
        ended_at: now,
        history: vec![
            IterationInput {
                iteration: 1,
                student_status: StudentStatusInput::AskMentor,
                current_step: "Step 2: Initialize the Project".to_string(),
                problem: Some("Command not found: npm".to_string()),
                question_for_mentor: Some("How do I install npm or Node.js?".to_string()),
                reason: Some("missing_dependency".to_string()),
                summary: "Created directory but cannot initialize npm project".to_string(),
                files_created: vec!["my-counter/".to_string()],
                commands_run: vec!["mkdir my-counter".to_string(), "npm init -y".to_string()],
                started_at: now - chrono::Duration::minutes(4),
                ended_at: now - chrono::Duration::minutes(3),
            },
            IterationInput {
                iteration: 2,
                student_status: StudentStatusInput::AskMentor,
                current_step: "Step 4: Configure the Executable".to_string(),
                problem: Some("Instructions unclear about which file to update".to_string()),
                question_for_mentor: Some("Which configuration file should I update?".to_string()),
                reason: Some("ambiguous_instruction".to_string()),
                summary: "Created counter.js but stuck on configuration".to_string(),
                files_created: vec!["my-counter/counter.js".to_string()],
                commands_run: vec![],
                started_at: now - chrono::Duration::minutes(3),
                ended_at: now - chrono::Duration::minutes(2),
            },
            IterationInput {
                iteration: 3,
                student_status: StudentStatusInput::AskMentor,
                current_step: "Step 5: Test Your Counter".to_string(),
                problem: Some("Permission denied when running ./counter.js".to_string()),
                question_for_mentor: Some("How do I make the script executable?".to_string()),
                reason: Some("command_failure".to_string()),
                summary: "Cannot run script due to permissions".to_string(),
                files_created: vec![],
                commands_run: vec!["./counter.js show".to_string()],
                started_at: now - chrono::Duration::minutes(2),
                ended_at: now - chrono::Duration::minutes(1),
            },
            IterationInput {
                iteration: 4,
                student_status: StudentStatusInput::Completed,
                current_step: "Step 6: Install Globally".to_string(),
                problem: None,
                question_for_mentor: None,
                reason: None,
                summary: "Tutorial completed after chmod fix".to_string(),
                files_created: vec![],
                commands_run: vec![
                    "chmod +x counter.js".to_string(),
                    "./counter.js show".to_string(),
                    "npm link".to_string(),
                ],
                started_at: now - chrono::Duration::minutes(1),
                ended_at: now,
            },
        ],
        mentor_notes: vec![
            MentorNoteInput {
                iteration: 1,
                question: "How do I install npm or Node.js?".to_string(),
                answer: "Install Node.js from nodejs.org. The tutorial is missing Node.js as a prerequisite.".to_string(),
                timestamp: now - chrono::Duration::minutes(3),
            },
            MentorNoteInput {
                iteration: 2,
                question: "Which configuration file should I update?".to_string(),
                answer: "Add a 'bin' field to package.json. The tutorial is unclear about this.".to_string(),
                timestamp: now - chrono::Duration::minutes(2),
            },
            MentorNoteInput {
                iteration: 3,
                question: "How do I make the script executable?".to_string(),
                answer: "Run chmod +x counter.js. The tutorial should mention this step.".to_string(),
                timestamp: now - chrono::Duration::minutes(1),
            },
        ],
    };

    // Generate report
    let generator = ReportGenerator::new(input);
    let report = generator.generate();

    // Verify report structure
    assert_eq!(report.tutorial_name, "sample-tutorial");
    assert_eq!(report.summary.iterations, 4);
    assert_eq!(report.summary.status, ReportStatus::Completed);

    // Verify gaps were extracted
    assert_eq!(
        report.gaps.len(),
        3,
        "Expected 3 gaps (iterations 1-3 asked mentor)"
    );

    // Generate markdown report
    let md_generator = MarkdownGenerator::new(&report);
    let markdown = md_generator.generate();

    // Verify markdown contains expected sections
    assert!(
        markdown.contains("# SMILE Validation Report"),
        "Should have main title"
    );
    assert!(
        markdown.contains("## Summary"),
        "Should have summary section"
    );
    assert!(
        markdown.contains("## Documentation Gaps"),
        "Should have gaps section"
    );
    assert!(
        markdown.contains("## Timeline"),
        "Should have timeline section"
    );
    assert!(
        markdown.contains("## Recommendations"),
        "Should have recommendations section"
    );

    // Verify gap content
    assert!(
        markdown.contains("npm"),
        "Should mention npm in gap description"
    );
    assert!(
        markdown.contains("chmod") || markdown.contains("executable"),
        "Should mention chmod or executable"
    );

    // Verify timeline entries
    assert!(
        markdown.contains("Step 2"),
        "Should mention Step 2 in timeline"
    );
    assert!(
        markdown.contains("Step 4"),
        "Should mention Step 4 in timeline"
    );
    assert!(
        markdown.contains("Step 5"),
        "Should mention Step 5 in timeline"
    );

    // Verify mentor notes are referenced
    assert!(
        markdown.contains("Node.js"),
        "Should mention Node.js from mentor note"
    );
}

/// Tests that report correctly identifies when a tutorial completes vs times out.
#[test]
fn test_report_status_variations() {
    let now = chrono::Utc::now();

    // Test completed status
    let completed_input = ReportInput {
        tutorial_name: "test".to_string(),
        tutorial_path: "/test.md".to_string(),
        status: ReportStatus::Completed,
        iterations: 1,
        started_at: now,
        ended_at: now,
        history: vec![IterationInput {
            iteration: 1,
            student_status: StudentStatusInput::Completed,
            current_step: "Final Step".to_string(),
            problem: None,
            question_for_mentor: None,
            reason: None,
            summary: "Done".to_string(),
            files_created: vec![],
            commands_run: vec![],
            started_at: now,
            ended_at: now,
        }],
        mentor_notes: vec![],
    };

    let report = ReportGenerator::new(completed_input).generate();
    assert_eq!(report.summary.status, ReportStatus::Completed);
    assert!(
        report.gaps.is_empty(),
        "Completed tutorial should have no gaps"
    );

    // Test max iterations status
    let max_iter_input = ReportInput {
        tutorial_name: "test".to_string(),
        tutorial_path: "/test.md".to_string(),
        status: ReportStatus::MaxIterations,
        iterations: 5,
        started_at: now,
        ended_at: now,
        history: vec![
            IterationInput {
                iteration: 1,
                student_status: StudentStatusInput::AskMentor,
                current_step: "Step 1".to_string(),
                problem: Some("Stuck".to_string()),
                question_for_mentor: Some("Help!".to_string()),
                reason: None,
                summary: "Stuck".to_string(),
                files_created: vec![],
                commands_run: vec![],
                started_at: now,
                ended_at: now,
            };
            5
        ],
        mentor_notes: vec![],
    };

    let report = ReportGenerator::new(max_iter_input).generate();
    assert_eq!(report.summary.status, ReportStatus::MaxIterations);
    assert_eq!(report.gaps.len(), 5, "Should have one gap per iteration");

    // Test blocker status
    let blocker_input = ReportInput {
        tutorial_name: "test".to_string(),
        tutorial_path: "/test.md".to_string(),
        status: ReportStatus::Blocker,
        iterations: 1,
        started_at: now,
        ended_at: now,
        history: vec![IterationInput {
            iteration: 1,
            student_status: StudentStatusInput::CannotComplete,
            current_step: "Step 1".to_string(),
            problem: Some("Fatal error".to_string()),
            question_for_mentor: None,
            reason: Some("Missing critical dependency".to_string()),
            summary: "Cannot proceed".to_string(),
            files_created: vec![],
            commands_run: vec![],
            started_at: now,
            ended_at: now,
        }],
        mentor_notes: vec![],
    };

    let report = ReportGenerator::new(blocker_input).generate();
    assert_eq!(report.summary.status, ReportStatus::Blocker);
    assert!(
        report.has_critical_gaps(),
        "Blocker should have critical gap"
    );
}
