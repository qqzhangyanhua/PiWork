//! Characterization: Pi launch arguments and capability declaration (#17).
//!
//! Seams under test:
//! - `PiRunArguments` for production launch flags and per-mode tool allowlists
//! - `PiEngineAdapter::capabilities` for the permission-request declaration
//!
//! These tests lock today's ground truth, including the known gap that Balanced
//! and AutoExecute admit the same builtin tools. They do not parse source text
//! and do not start a Pi sidecar. `PiEngineAdapter` construction is fail-closed
//! outside Windows/macOS, so the capability assertion is gated to those targets.

use std::path::Path;

use piwork_lib::{domain::work::PermissionMode, engine::pi::PiRunArguments};

fn launch_arguments(permission_mode: PermissionMode) -> PiRunArguments {
    PiRunArguments::new(
        Path::new("/workspace"),
        Path::new("/sessions/work-1"),
        "agent-session-1",
        "agent-model",
        permission_mode,
    )
}

fn tools_allowlist(arguments: &PiRunArguments) -> Vec<&str> {
    arguments
        .values()
        .windows(2)
        .find(|pair| pair[0] == "--tools")
        .expect("production Pi launch arguments include --tools")[1]
        .split(',')
        .collect()
}

fn permission_request_switches(arguments: &PiRunArguments) -> Vec<&str> {
    arguments
        .values()
        .iter()
        .map(String::as_str)
        .filter(|value| value.starts_with('-'))
        .filter(|value| {
            value
                .trim_start_matches('-')
                .to_ascii_lowercase()
                .contains("permission")
        })
        .collect()
}

#[test]
fn production_launch_arguments_auto_approve_and_omit_permission_request_switches() {
    for mode in [
        PermissionMode::AskEveryStep,
        PermissionMode::Balanced,
        PermissionMode::AutoExecute,
    ] {
        let arguments = launch_arguments(mode);
        let values = arguments.values();

        assert!(
            values.iter().any(|value| value == "--approve"),
            "production Pi launch still auto-approves tools via --approve ({mode:?})"
        );
        assert!(
            !values
                .iter()
                .any(|value| value == "--no-approve" || value == "-na"),
            "production Pi launch must not pass --no-approve ({mode:?})"
        );
        assert_eq!(
            permission_request_switches(&arguments),
            Vec::<&str>::new(),
            "production Pi launch currently has no permission-request switch ({mode:?}): {values:?}"
        );
    }
}

#[test]
fn ask_every_step_launch_allowlist_is_read_only_builtin_tools() {
    assert_eq!(
        tools_allowlist(&launch_arguments(PermissionMode::AskEveryStep)),
        ["read", "grep", "find", "ls"]
    );
}

#[test]
fn balanced_launch_allowlist_admits_all_seven_builtin_tools() {
    assert_eq!(
        tools_allowlist(&launch_arguments(PermissionMode::Balanced)),
        ["read", "grep", "find", "ls", "edit", "write", "bash"]
    );
}

#[test]
fn auto_execute_launch_allowlist_admits_all_seven_builtin_tools() {
    assert_eq!(
        tools_allowlist(&launch_arguments(PermissionMode::AutoExecute)),
        ["read", "grep", "find", "ls", "edit", "write", "bash"]
    );
}

#[test]
fn balanced_and_auto_execute_currently_share_the_same_launch_allowlist() {
    let balanced_arguments = launch_arguments(PermissionMode::Balanced);
    let auto_execute_arguments = launch_arguments(PermissionMode::AutoExecute);

    assert_eq!(
        tools_allowlist(&balanced_arguments),
        tools_allowlist(&auto_execute_arguments),
        "Balanced and AutoExecute currently admit the same Pi builtin tools; this is documented ground truth, not an intended three-way split"
    );
}

#[cfg(any(windows, target_os = "macos"))]
#[tokio::test]
async fn pi_adapter_declares_permission_requests_closed() {
    use std::sync::Arc;

    use piwork_lib::{
        engine::{EngineAdapter, pi::PiEngineAdapter},
        model::{ModelConfigurationRepository, ModelService},
        storage::sqlite::Database,
    };

    let database = Database::open_in_memory().await.unwrap();
    let model_service = Arc::new(
        ModelService::production(ModelConfigurationRepository::new(database.pool().clone()))
            .unwrap(),
    );
    let temporary_directory = tempfile::tempdir().unwrap();
    let adapter = PiEngineAdapter::production_with_executable(
        model_service,
        temporary_directory.path().join("sessions"),
        temporary_directory.path().join("runtime"),
        Some(std::env::current_exe().unwrap()),
    )
    .unwrap();

    assert!(
        !adapter.capabilities().permission_requests,
        "production Pi adapter currently declares no permission-request handshake"
    );
}
