use std::{collections::BTreeSet, future::Future, sync::Arc};

#[cfg(all(test, windows))]
#[link(name = "resource", kind = "static")]
unsafe extern "C" {}

use tauri::{Manager, path::BaseDirectory};

pub mod agent;
pub mod app_state;
pub mod assignment;
pub mod capability;
pub mod collaboration;
pub mod connectors;
pub mod delivery;
pub mod document_runtime;
pub mod domain;
pub mod engine;
pub mod environment;
pub mod error;
pub mod execution;
pub mod extensions;
pub mod memory;
pub mod model;
pub mod paths;
pub mod resource;
pub mod secret;
pub mod storage;
pub mod work;
pub mod workspace;

type StartupError = Box<dyn std::error::Error>;
type StartupResult<T> = Result<T, StartupError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupFailureDecision {
    Retry,
    Exit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StartupFailureNotice {
    code: &'static str,
    title: &'static str,
    body: &'static str,
}

fn startup_failure_notice() -> StartupFailureNotice {
    StartupFailureNotice {
        code: "PIWORK-STARTUP-001",
        title: "CoDo could not start",
        body: "CoDo could not prepare its local data. Select Retry to try again or Cancel to exit. Diagnostic logs and application data are stored in your operating system application data folders.",
    }
}

#[cfg(windows)]
fn prompt_startup_failure(notice: &StartupFailureNotice) -> StartupResult<StartupFailureDecision> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDCANCEL, IDRETRY, MB_ICONERROR, MB_RETRYCANCEL, MB_SETFOREGROUND, MessageBoxW,
    };

    let title = format!("{} ({})", notice.title, notice.code);
    let body = format!("{}\n\nError code: {}", notice.body, notice.code);
    let title = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let body = body
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: Both strings are NUL-terminated and remain alive for the duration of the call.
    let decision = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_ICONERROR | MB_RETRYCANCEL | MB_SETFOREGROUND,
        )
    };
    match decision {
        IDRETRY => Ok(StartupFailureDecision::Retry),
        IDCANCEL => Ok(StartupFailureDecision::Exit),
        _ => Err(std::io::Error::other("native startup failure dialog was unavailable").into()),
    }
}

#[cfg(not(windows))]
fn prompt_startup_failure(notice: &StartupFailureNotice) -> StartupResult<StartupFailureDecision> {
    eprintln!("CoDo startup failed ({}).", notice.code);
    Err(std::io::Error::other("native startup failure dialog is unavailable").into())
}

trait SecondInstanceWindow {
    type Error;

    fn is_visible(&self) -> Result<bool, Self::Error>;
    fn set_focus(&self) -> Result<(), Self::Error>;
}

impl<R: tauri::Runtime> SecondInstanceWindow for tauri::WebviewWindow<R> {
    type Error = tauri::Error;

    fn is_visible(&self) -> Result<bool, Self::Error> {
        self.is_visible()
    }

    fn set_focus(&self) -> Result<(), Self::Error> {
        self.set_focus()
    }
}

fn focus_visible_main_window<W: SecondInstanceWindow>(window: &W) -> Result<(), W::Error> {
    if window.is_visible()? {
        window.set_focus()?;
    }
    Ok(())
}

fn production_agent_tools() -> BTreeSet<String> {
    engine::pi::production_pi_tool_ids()
        .iter()
        .copied()
        .map(String::from)
        .collect()
}

fn production_agent_engine_capabilities() -> BTreeSet<String> {
    // B-stage executable packs currently declare no engine capability requirements.
    BTreeSet::new()
}

async fn orchestrate_startup<T, E, Prepare, PrepareFuture, Assemble, Show, Prompt>(
    mut prepare: Prepare,
    mut assemble: Assemble,
    show: Show,
    mut prompt: Prompt,
) -> Result<(), E>
where
    Prepare: FnMut() -> PrepareFuture,
    PrepareFuture: Future<Output = Result<T, E>>,
    Assemble: FnMut(T) -> Result<(), E>,
    Show: FnOnce() -> Result<(), E>,
    Prompt: FnMut(&StartupFailureNotice) -> Result<StartupFailureDecision, E>,
{
    loop {
        let attempt = match prepare().await {
            Ok(prepared) => assemble(prepared),
            Err(error) => Err(error),
        };
        match attempt {
            Ok(()) => return show(),
            Err(error) => match prompt(&startup_failure_notice())? {
                StartupFailureDecision::Retry => continue,
                StartupFailureDecision::Exit => return Err(error),
            },
        }
    }
}

fn application_builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = focus_visible_main_window(&window);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let paths = paths::AppPaths::from_resolver(app.path())?;
            // Tauri's setup callback runs synchronously on the event-loop thread. Bridge the
            // complete startup sequence once. The configured main window remains hidden until
            // migrations, recovery, and managed state assembly all succeed.
            let database_path = paths.database_path().to_path_buf();
            let app_handle = app.handle().clone();
            let engine_sessions_dir = paths.engine_sessions_dir();
            let runtime_dir = paths.runtime_dir();
            let resources_dir = paths.resources_dir();
            let resource_cache_dir = paths.resource_cache_dir();
            let bundled_pi = app
                .path()
                .resolve("pi-sidecar/dist/piwork-pi.js", BaseDirectory::Resource)
                .ok();
            let bundled_host_tools = app
                .path()
                .resolve("piwork-host-tools.ts", BaseDirectory::Resource)
                .ok();
            let bundled_web_access = app
                .path()
                .resolve(
                    "pi-sidecar/builtin-extensions/pi-web-access/node_modules/pi-web-access/index.ts",
                    BaseDirectory::Resource,
                )
                .ok();
            let observer = engine::activity_observer::ActivityObserverHandle::in_process();
            tauri::async_runtime::block_on(orchestrate_startup(
                || {
                    let database_path = database_path.clone();
                    let app_handle = app_handle.clone();
                    let observer = observer.clone();
                    let resources_dir = resources_dir.clone();
                    let resource_cache_dir = resource_cache_dir.clone();
                    async move {
                        let database = storage::sqlite::Database::open(database_path).await?;
                        workspace::WorkspaceRepository::new(database.pool().clone())
                            .reconcile_legacy_paths()
                            .await?;
                        let repository =
                            work::repository::WorkRepository::new(database.pool().clone());
                        let agent_repository =
                            agent::repository::AgentRepository::new(database.pool().clone());
                        let publisher = Arc::new(
                            engine::publisher::TauriEventPublisher::with_observer(
                                app_handle,
                                observer.clone(),
                            ),
                        );
                        let assignment_sink: Arc<dyn assignment::repository::AssignmentEventSink> =
                            publisher.clone();
                        let assignment_repository = assignment::repository::AssignmentRepository::initialize_with_event_sink(
                            database.pool().clone(),
                            assignment_sink,
                        )
                        .await?;
                        // Recover assignments orphaned by a previous process before
                        // the window is shown or the scheduler starts dispatching.
                        assignment_repository.recover_orphans(&[]).await?;
                        work::service::WorkService::new(repository.clone())
                            .recover_interrupted_runs()
                            .await?;
                        repository.rebuild_work_statuses().await?;
                        let model_repository =
                            model::ModelConfigurationRepository::new(database.pool().clone());
                        let resource_repository =
                            resource::repository::ResourceRepository::new(database.pool().clone());
                        let blob_store = Arc::new(resource::local_blob_store::LocalBlobStore::new(
                            resources_dir,
                        ));
                        let resource_service = Arc::new(resource::service::ResourceService::new(
                            resource_repository,
                            blob_store,
                            resource_cache_dir,
                        ));
                        resource_service.recover_interrupted_imports().await?;
                        Ok::<_, StartupError>((
                            repository,
                            model_repository,
                            resource_service,
                            agent_repository,
                            publisher,
                            assignment_repository,
                            database.pool().clone(),
                        ))
                    }
                },
                |(
                    repository,
                    model_repository,
                    resource_service,
                    agent_repository,
                    publisher,
                    assignment_repository,
                    pool,
                )| -> StartupResult<()> {
                    let model_service =
                        Arc::new(model::ModelService::production(model_repository)?);
                    let extension_service = Arc::new(extensions::ExtensionService::new(
                        pool.clone(),
                        bundled_web_access.clone(),
                    )?);
                    let connector_service = Arc::new(connectors::ConnectorService::new(
                        pool.clone(),
                        Some(app.handle().clone()),
                    ));
                    let memory_service =
                        Arc::new(memory::WorkspaceMemoryService::production(pool.clone())?);
                    let host_tool_extension = bundled_host_tools.clone().ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "bundled PiWork host tools extension is unavailable",
                        )
                    })?;
                    let engine = Arc::new(
                        engine::pi::PiEngineAdapter::production_with_executable(
                            Arc::clone(&model_service),
                            engine_sessions_dir.clone(),
                            runtime_dir.clone(),
                            bundled_pi.clone(),
                        )?
                        .with_host_tool_extension(host_tool_extension)
                        .with_extension_service(Arc::clone(&extension_service)),
                    );
                    if !app.manage(observer.clone()) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "PiWork activity observer is already managed",
                        )
                        .into());
                    }
                    if !app.manage(assignment_repository.clone()) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "PiWork assignment repository is already managed",
                        )
                        .into());
                    }
                    let host_tool_registry =
                        Arc::new(collaboration::tool_bridge::HostToolRegistry::new());
                    let host_tool_endpoint = Arc::new(std::sync::OnceLock::new());
                    let capability_broker = capability::CapabilityBroker::new(pool.clone());
                    let scheduler = assignment::scheduler::AssignmentScheduler::new(
                        assignment_repository.clone(),
                        repository.clone(),
                        agent_repository.clone(),
                        Arc::clone(&engine) as Arc<dyn engine::EngineAdapter>,
                        Arc::clone(&publisher) as Arc<dyn engine::publisher::EventPublisher>,
                        format!("piwork-scheduler-{}", uuid::Uuid::new_v4()),
                        "Pi",
                    )
                    .with_host_tools(engine::harness::HostToolBridgeConfig {
                        registry: Arc::clone(&host_tool_registry),
                        endpoint: Arc::clone(&host_tool_endpoint),
                    })
                    .with_capability_broker(capability_broker.clone())
                    .with_extension_service(Arc::clone(&extension_service))
                    .with_memory_service(Arc::clone(&memory_service));
                    let scheduler_handle = scheduler.spawn();

                    // Bind the loopback Host Tool Bridge and publish its endpoint
                    // so the Harness can issue per-Run leases. The endpoint is set
                    // exactly once, before the window is shown and any Run runs.
                    let lead_tools = collaboration::service::LeadToolService::new(
                        assignment_repository.clone(),
                        repository.clone(),
                        agent_repository.clone(),
                        scheduler_handle.clone(),
                    );
                    let member_tools = collaboration::service::MemberResultService::new(
                        assignment_repository.clone(),
                        scheduler_handle.clone(),
                    )
                    .with_memory(collaboration::memory::MemoryService::new(pool.clone()));
                    let dispatcher = collaboration::service::HostToolDispatcher::new(
                        lead_tools,
                        member_tools,
                    )
                    .with_connector_service(Arc::clone(&connector_service));
                    let dispatch: Arc<collaboration::tool_server::ToolDispatch> =
                        Arc::new(move |tool, context, arguments| {
                            dispatcher.dispatch(tool, context, arguments)
                        });
                    let host_tool_server =
                        collaboration::tool_server::HostToolServer::bind_with_capability_broker(
                            Arc::clone(&host_tool_registry),
                            dispatch,
                            capability_broker,
                        )?;
                    let _ = host_tool_endpoint.set(host_tool_server.endpoint().to_owned());
                    if !app.manage(host_tool_server) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "PiWork host tool bridge is already managed",
                        )
                        .into());
                    }
                    if !app.manage(collaboration::memory::MemoryService::new(pool.clone())) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "PiWork memory service is already managed",
                        )
                        .into());
                    }

                    let assignment_service = Arc::new(assignment::service::AssignmentService::new(
                        assignment_repository,
                        repository.clone(),
                        scheduler_handle.clone(),
                    ));
                    let execution_coordinator = Arc::new(execution::ExecutionCoordinator::new(
                        Arc::clone(&assignment_service),
                        repository.clone(),
                        scheduler_handle,
                    ));
                    let service = Arc::new(work::service::WorkService::new(repository));
                    let agent_service = Arc::new(agent::service::AgentService::new(
                        agent_repository,
                        production_agent_tools(),
                        production_agent_engine_capabilities(),
                    ));
                    if !app.manage(
                        app_state::AppState::with_services(
                            service,
                            model_service,
                            resource_service,
                            agent_service,
                        )
                        .with_assignment_service(assignment_service)
                        .with_execution_coordinator(execution_coordinator)
                        .with_extension_service(extension_service)
                        .with_connector_service(Arc::clone(&connector_service))
                        .with_memory_service(Arc::clone(&memory_service)),
                    ) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "PiWork application state is already managed",
                        )
                        .into());
                    }
                    connector_service.start_poller();
                    memory_service.start_worker();
                    Ok(())
                },
                || -> StartupResult<()> {
                    app.get_webview_window("main")
                        .ok_or(tauri::Error::WebviewNotFound)?
                        .show()?;
                    Ok(())
                },
                prompt_startup_failure,
            ))
        })
        .invoke_handler(tauri::generate_handler![
            work::commands::get_default_project_directory,
            work::commands::create_work,
            work::commands::list_works,
            work::commands::get_work,
            work::commands::list_project_files,
            work::commands::start_work,
            work::commands::stop_work,
            work::commands::archive_work,
            work::commands::restore_work,
            assignment::commands::drain_assignment_event_outbox,
            assignment::commands::list_work_assignments,
            assignment::commands::queue_work_input,
            assignment::commands::confirm_assignment_recovery,
            assignment::commands::interrupt_and_replace,
            collaboration::commands::resolve_memory_candidate,
            collaboration::commands::list_memory_candidates,
            model::commands::get_model_configuration_status,
            model::commands::list_model_configurations,
            model::commands::test_model_connection,
            model::commands::test_saved_model_configuration,
            model::commands::save_model_configuration,
            model::commands::activate_model_configuration,
            model::commands::select_model_for_configuration,
            resource::commands::import_resources,
            resource::commands::list_work_resources,
            resource::commands::get_resource_thumbnail,
            resource::commands::detach_draft_resource,
            environment::commands::get_runtime_status,
            agent::commands::list_agent_instances,
            agent::commands::list_capability_packs,
            agent::commands::get_work_team,
            agent::commands::validate_agent_assembly,
            agent::commands::save_agent_copy,
            agent::commands::add_work_member,
            extensions::commands::list_extensions,
            extensions::commands::search_community_extensions,
            extensions::commands::set_extension_agent_enabled,
            extensions::commands::get_web_access_settings,
            extensions::commands::save_web_access_settings,
            memory::commands::get_memory_settings,
            memory::commands::save_memory_settings,
            memory::commands::test_memory_connection,
            memory::commands::list_workspace_memory_bindings,
            memory::commands::save_workspace_memory_binding,
            memory::commands::drain_memory_capture_outbox,
            connectors::commands::list_email_connectors,
            connectors::commands::resolve_connector_icon,
            connectors::commands::save_email_connector,
            connectors::commands::test_email_connector,
            connectors::commands::set_email_connector_enabled,
            connectors::commands::delete_email_connector,
            connectors::commands::set_connector_work_grant,
            connectors::commands::set_connector_agent_grant,
            connectors::commands::list_email_metadata,
            connectors::commands::list_app_notifications,
            connectors::commands::mark_app_notification_read,
            connectors::commands::clear_app_notification,
            connectors::commands::list_pending_connector_actions,
            connectors::commands::resolve_pending_connector_action,
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    application_builder()
        .run(tauri::generate_context!())
        .expect("failed to run PiWork");
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        sync::{Arc, Mutex},
    };

    struct TestSecondInstanceWindow {
        visible: bool,
        focus_count: Cell<usize>,
    }

    impl TestSecondInstanceWindow {
        fn new(visible: bool) -> Self {
            Self {
                visible,
                focus_count: Cell::new(0),
            }
        }
    }

    impl super::SecondInstanceWindow for TestSecondInstanceWindow {
        type Error = &'static str;

        fn is_visible(&self) -> Result<bool, Self::Error> {
            Ok(self.visible)
        }

        fn set_focus(&self) -> Result<(), Self::Error> {
            self.focus_count.set(self.focus_count.get() + 1);
            Ok(())
        }
    }

    #[test]
    fn application_builder_typechecks() {
        let _ = super::application_builder;
    }

    #[test]
    fn production_registers_every_resource_command() {
        let source = include_str!("lib.rs");
        for command in [
            "resource::commands::import_resources",
            "resource::commands::list_work_resources",
            "resource::commands::get_resource_thumbnail",
            "resource::commands::detach_draft_resource",
        ] {
            assert!(source.contains(command), "missing {command}");
        }
    }

    #[test]
    fn production_registers_every_agent_command() {
        let source = include_str!("lib.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for command in [
            "agent::commands::list_agent_instances",
            "agent::commands::list_capability_packs",
            "agent::commands::get_work_team",
            "agent::commands::validate_agent_assembly",
            "agent::commands::save_agent_copy",
            "agent::commands::add_work_member",
        ] {
            assert!(production.contains(command), "missing {command}");
        }
    }

    #[tokio::test]
    async fn production_agent_allowlists_match_migrated_executable_packs() {
        let database = crate::storage::sqlite::Database::open_in_memory()
            .await
            .unwrap();
        let requirements: Vec<(String, String)> = sqlx::query_as(
            "SELECT required_tools_json, required_engine_capabilities_json \
             FROM capability_packs WHERE status = 'executable'",
        )
        .fetch_all(database.pool())
        .await
        .unwrap();
        let mut tools = std::collections::BTreeSet::new();
        let mut engine_capabilities = std::collections::BTreeSet::new();
        for (required_tools, required_engine_capabilities) in requirements {
            tools.extend(serde_json::from_str::<Vec<String>>(&required_tools).unwrap());
            engine_capabilities.extend(
                serde_json::from_str::<Vec<String>>(&required_engine_capabilities).unwrap(),
            );
        }

        let arguments = crate::engine::pi::PiRunArguments::new(
            std::path::Path::new("."),
            std::path::Path::new("."),
            "allowlist-test",
            "allowlist-test-model",
            crate::domain::work::PermissionMode::Balanced,
        );
        let runtime_tools = arguments
            .values()
            .windows(2)
            .find(|pair| pair[0] == "--tools")
            .expect("Pi runtime arguments must include --tools")[1]
            .split(',')
            .map(String::from)
            .collect::<std::collections::BTreeSet<_>>();
        let production_source_tools: std::collections::BTreeSet<String> =
            crate::engine::pi::production_pi_tool_ids()
                .iter()
                .map(|tool| String::from(*tool))
                .collect();

        assert_eq!(runtime_tools, tools);
        assert_eq!(production_source_tools, tools);
        assert_eq!(super::production_agent_tools(), tools);
        assert_eq!(
            super::production_agent_engine_capabilities(),
            engine_capabilities,
        );
    }

    #[test]
    fn production_agent_allowlist_delegates_to_pi_runtime_tool_source() {
        let source = include_str!("lib.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();

        assert!(production.contains("engine::pi::production_pi_tool_ids()"));
        assert!(
            !production
                .contains("[\"read\", \"grep\", \"find\", \"ls\", \"edit\", \"write\", \"bash\"]")
        );
    }

    #[test]
    fn production_uses_pi_rpc_instead_of_direct_model_completion() {
        let source = include_str!("lib.rs");
        let assembly = source
            .split("fn application_builder()")
            .nth(1)
            .unwrap()
            .split("#[cfg_attr(mobile")
            .next()
            .unwrap();
        assert!(assembly.contains("engine::pi::PiEngineAdapter::production_with_executable"));
        assert!(!assembly.contains("ConfiguredModelEngineAdapter::new"));
    }

    #[test]
    fn production_manages_the_publishers_shared_activity_observer() {
        let source = include_str!("lib.rs");
        let assembly = source
            .split("fn application_builder()")
            .nth(1)
            .unwrap()
            .split("#[cfg_attr(mobile")
            .next()
            .unwrap();
        let observer = assembly
            .find("engine::activity_observer::ActivityObserverHandle::in_process()")
            .expect("production activity observer construction is missing");
        let publisher = assembly
            .find("engine::publisher::TauriEventPublisher::with_observer")
            .expect("publisher does not receive the shared activity observer");
        let manage = assembly
            .find("app.manage(observer.clone())")
            .expect("shared activity observer is not retained in Tauri managed state");
        let erase = assembly
            .find("assignment::scheduler::AssignmentScheduler::new")
            .expect("publisher is not erased into the assignment scheduler");

        assert!(observer < publisher && publisher < manage && manage < erase);
        assert!(assembly[publisher..manage].contains("observer.clone()"));
        let management = &assembly[manage..erase];
        assert!(management.contains("std::io::ErrorKind::AlreadyExists"));
        assert!(management.contains("PiWork activity observer is already managed"));
    }

    #[test]
    fn production_initializes_and_manages_the_assignment_outbox_before_readiness() {
        let source = include_str!("lib.rs");
        let assembly = source
            .split("fn application_builder()")
            .nth(1)
            .unwrap()
            .split("#[cfg_attr(mobile")
            .next()
            .unwrap();
        let publisher = assembly
            .find("engine::publisher::TauriEventPublisher::with_observer")
            .expect("production assignment sink does not use the shared Tauri publisher");
        let initialize = assembly
            .find("assignment::repository::AssignmentRepository::initialize_with_event_sink")
            .expect("production startup does not recover and drain the assignment outbox");
        let manage = assembly
            .find("app.manage(assignment_repository.clone())")
            .expect("the initialized assignment repository is not retained in managed state");
        let drain_command = assembly
            .find("assignment::commands::drain_assignment_event_outbox")
            .expect("the frontend-ready outbox drain command is not registered");
        let show = assembly
            .find(".show()?")
            .expect("the main window readiness boundary is missing");

        assert!(publisher < initialize && initialize < manage && manage < show);
        assert!(manage < drain_command);
    }

    #[test]
    fn every_windows_binary_declares_the_gui_subsystem() {
        let source = include_str!("main.rs");
        assert!(source.starts_with("#![cfg_attr(windows, windows_subsystem = \"windows\")]"));
        assert!(!source.contains("not(debug_assertions)"));
    }

    #[test]
    fn configured_main_window_starts_hidden() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let main_window = config["app"]["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["label"] == "main")
            .unwrap();

        assert_eq!(main_window["visible"], false);
    }

    #[test]
    fn bundle_maps_the_root_notice_to_a_stable_resource_name() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();

        assert_eq!(config["bundle"]["resources"]["../NOTICE"], "NOTICE");
        assert_eq!(
            config["bundle"]["resources"]["binaries/pi-sidecar"],
            "pi-sidecar"
        );
        assert_eq!(
            config["bundle"]["resources"]["binaries/document-runtime"],
            "document-runtime"
        );
        assert!(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../NOTICE")
                .is_file()
        );
    }

    #[test]
    fn macos_bundle_target_is_dmg_while_windows_keeps_nsis() {
        let windows: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(windows["bundle"]["targets"], serde_json::json!(["nsis"]));

        let macos_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.macos.conf.json");
        let macos: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&macos_path).expect("macOS packaging config"),
        )
        .unwrap();
        assert_eq!(macos["bundle"]["targets"], serde_json::json!(["dmg"]));
        let before = macos["build"]["beforeBuildCommand"]
            .as_str()
            .expect("macOS beforeBuildCommand");
        assert!(
            !before.to_ascii_lowercase().contains("powershell"),
            "macOS packaging must not depend on PowerShell"
        );
    }

    #[test]
    fn darwin_sidecar_node_is_gitignored_and_windows_node_exe_remains_versioned() {
        let gitignore = include_str!("../../.gitignore");
        assert!(
            gitignore
                .lines()
                .any(|line| line == "/src-tauri/binaries/pi-sidecar/node"),
            "darwin Node must not enter git"
        );
        assert!(
            gitignore
                .lines()
                .any(|line| line == "/src-tauri/binaries/document-runtime/piwork-document-runtime"),
            "macOS document-runtime helper must not enter git"
        );
        assert!(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("binaries/pi-sidecar/node.exe")
                .is_file(),
            "Windows bundled Node remains in the repository"
        );
    }

    #[test]
    fn single_instance_plugin_is_registered_before_user_setup() {
        let manifest = include_str!("../Cargo.toml");
        assert!(manifest.contains("tauri-plugin-single-instance"));

        let source = include_str!("lib.rs");
        let plugin = source
            .find(".plugin(tauri_plugin_single_instance::init")
            .expect("single-instance plugin registration is missing");
        let setup = source
            .find(".setup(|app|")
            .expect("application setup hook is missing");
        assert!(plugin < setup, "single-instance must be registered first");
    }

    #[test]
    fn second_instance_does_not_focus_a_hidden_startup_window() {
        let window = TestSecondInstanceWindow::new(false);

        super::focus_visible_main_window(&window).unwrap();

        assert_eq!(window.focus_count.get(), 0);
    }

    #[test]
    fn second_instance_focuses_an_already_visible_main_window() {
        let window = TestSecondInstanceWindow::new(true);

        super::focus_visible_main_window(&window).unwrap();

        assert_eq!(window.focus_count.get(), 1);
    }

    #[tokio::test]
    async fn startup_failure_retry_repeats_prepare_then_assembles_and_shows() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let recover_calls = Arc::clone(&calls);
        let attempt_count = Arc::clone(&attempts);
        let assemble_calls = Arc::clone(&calls);
        let show_calls = Arc::clone(&calls);
        let dialog_calls = Arc::clone(&calls);

        let result = super::orchestrate_startup(
            move || {
                let attempt = attempt_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let recover_calls = Arc::clone(&recover_calls);
                async move {
                    recover_calls.lock().unwrap().push("recover");
                    if attempt == 0 {
                        Err("raw SQL secret")
                    } else {
                        Ok("repository")
                    }
                }
            },
            |repository| {
                assert_eq!(repository, "repository");
                assemble_calls.lock().unwrap().push("assemble");
                Ok(())
            },
            || {
                show_calls.lock().unwrap().push("show");
                Ok(())
            },
            |notice| {
                assert_eq!(notice.code, "PIWORK-STARTUP-001");
                assert!(!notice.body.contains("raw SQL secret"));
                dialog_calls.lock().unwrap().push("dialog");
                Ok(super::StartupFailureDecision::Retry)
            },
        )
        .await;

        assert_eq!(result, Ok(()));
        assert_eq!(
            *calls.lock().unwrap(),
            vec!["recover", "dialog", "recover", "assemble", "show"]
        );
    }

    #[tokio::test]
    async fn successful_startup_shows_only_after_recovery_and_state_assembly() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recover_calls = Arc::clone(&calls);
        let assemble_calls = Arc::clone(&calls);
        let show_calls = Arc::clone(&calls);

        let result = super::orchestrate_startup(
            || {
                let recover_calls = Arc::clone(&recover_calls);
                async move {
                    recover_calls.lock().unwrap().push("recover");
                    Ok::<_, &str>("repository")
                }
            },
            |repository| {
                assert_eq!(repository, "repository");
                assemble_calls.lock().unwrap().push("assemble");
                Ok(())
            },
            || {
                show_calls.lock().unwrap().push("show");
                Ok(())
            },
            |_| Ok(super::StartupFailureDecision::Exit),
        )
        .await;

        assert_eq!(result, Ok(()));
        assert_eq!(*calls.lock().unwrap(), vec!["recover", "assemble", "show"]);
    }

    #[tokio::test]
    async fn startup_failure_exit_does_not_show_the_window() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recover_calls = Arc::clone(&calls);
        let assemble_calls = Arc::clone(&calls);
        let show_calls = Arc::clone(&calls);

        let result = super::orchestrate_startup(
            || {
                let recover_calls = Arc::clone(&recover_calls);
                async move {
                    recover_calls.lock().unwrap().push("recover");
                    Ok::<_, &str>(())
                }
            },
            |_| {
                assemble_calls.lock().unwrap().push("assemble");
                Err("assembly failed")
            },
            || {
                show_calls.lock().unwrap().push("show");
                Ok(())
            },
            |_| Ok(super::StartupFailureDecision::Exit),
        )
        .await;

        assert_eq!(result, Err("assembly failed"));
        assert_eq!(*calls.lock().unwrap(), vec!["recover", "assemble"]);
    }

    #[tokio::test]
    async fn window_show_error_is_propagated_after_state_assembly() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recover_calls = Arc::clone(&calls);
        let assemble_calls = Arc::clone(&calls);
        let show_calls = Arc::clone(&calls);

        let result = super::orchestrate_startup(
            || {
                let recover_calls = Arc::clone(&recover_calls);
                async move {
                    recover_calls.lock().unwrap().push("recover");
                    Ok::<_, &str>(())
                }
            },
            |_| {
                assemble_calls.lock().unwrap().push("assemble");
                Ok(())
            },
            || {
                show_calls.lock().unwrap().push("show");
                Err("show failed")
            },
            |_| Ok(super::StartupFailureDecision::Exit),
        )
        .await;

        assert_eq!(result, Err("show failed"));
        assert_eq!(*calls.lock().unwrap(), vec!["recover", "assemble", "show"]);
    }

    #[tokio::test]
    async fn every_repeated_failure_requires_an_explicit_user_decision() {
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let dialogs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let prepare_attempts = Arc::clone(&attempts);
        let dialog_attempts = Arc::clone(&dialogs);

        let result = super::orchestrate_startup(
            move || {
                prepare_attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async { Err::<(), _>("database failed") }
            },
            |_| Ok(()),
            || panic!("failed startup must not show"),
            move |_| {
                let dialog = dialog_attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(if dialog < 2 {
                    super::StartupFailureDecision::Retry
                } else {
                    super::StartupFailureDecision::Exit
                })
            },
        )
        .await;

        assert_eq!(result, Err("database failed"));
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
        assert_eq!(dialogs.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn startup_dialog_failure_fails_closed_without_showing() {
        let result = super::orchestrate_startup(
            || async { Err::<(), _>("raw migration secret") },
            |_| Ok(()),
            || panic!("dialog failure must not show"),
            |_| Err("dialog unavailable"),
        )
        .await;

        assert_eq!(result, Err("dialog unavailable"));
    }

    #[test]
    fn startup_failure_notice_is_actionable_and_contains_no_raw_error() {
        let notice = super::startup_failure_notice();
        let rendered = format!("{}\n{}\n{}", notice.code, notice.title, notice.body);

        assert!(rendered.contains("Retry"));
        assert!(rendered.contains("logs"));
        assert!(rendered.contains("application data"));
        assert!(!rendered.contains("raw SQL secret"));
        assert_eq!(notice.code, "PIWORK-STARTUP-001");
    }
}
