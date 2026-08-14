#![allow(linker_messages)] // Xberg's bundled static Tesseract selects a release CRT in debug builds.

use std::{collections::BTreeSet, future::Future, sync::Arc};

use tauri::{Manager, path::BaseDirectory};

pub mod agent;
pub mod app_state;
pub mod document_runtime;
pub mod domain;
pub mod engine;
pub mod environment;
pub mod error;
pub mod model;
pub mod paths;
pub mod resource;
pub mod storage;
pub mod work;

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
        title: "PiWork could not start",
        body: "PiWork could not prepare its local data. Select Retry to try again or Cancel to exit. Diagnostic logs and application data are stored in your operating system application data folders.",
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
    eprintln!("PiWork startup failed ({}).", notice.code);
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
    ["read", "grep", "find", "ls", "edit", "write", "bash"]
        .into_iter()
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
            let engine_sessions_dir = paths.engine_sessions_dir();
            let runtime_dir = paths.runtime_dir();
            let resources_dir = paths.resources_dir();
            let resource_cache_dir = paths.resource_cache_dir();
            let bundled_pi = app
                .path()
                .resolve("pi-sidecar/dist/piwork-pi.js", BaseDirectory::Resource)
                .ok();
            tauri::async_runtime::block_on(orchestrate_startup(
                || {
                    let database_path = database_path.clone();
                    let resources_dir = resources_dir.clone();
                    let resource_cache_dir = resource_cache_dir.clone();
                    async move {
                        let database = storage::sqlite::Database::open(database_path).await?;
                        let repository =
                            work::repository::WorkRepository::new(database.pool().clone());
                        let agent_repository =
                            agent::repository::AgentRepository::new(database.pool().clone());
                        work::service::WorkService::new(repository.clone())
                            .recover_interrupted_runs()
                            .await?;
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
                        ))
                    }
                },
                |(repository, model_repository, resource_service, agent_repository)| -> StartupResult<()> {
                    let model_service =
                        Arc::new(model::ModelService::production(model_repository)?);
                    let engine = Arc::new(engine::pi::PiEngineAdapter::production_with_executable(
                        Arc::clone(&model_service),
                        engine_sessions_dir.clone(),
                        runtime_dir.clone(),
                        bundled_pi.clone(),
                    )?);
                    let observer = engine::activity_observer::ActivityObserverHandle::in_process();
                    let publisher =
                        Arc::new(engine::publisher::TauriEventPublisher::with_observer(
                            app.handle().clone(),
                            observer.clone(),
                        ));
                    if !app.manage(observer.clone()) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "PiWork activity observer is already managed",
                        )
                        .into());
                    }
                    let supervisor = Arc::new(engine::supervisor::EngineSupervisor::new(
                        repository.clone(),
                        engine,
                        publisher,
                        "Pi",
                    ));
                    let service =
                        Arc::new(work::service::WorkService::with_supervisor_and_resources(
                            repository,
                            supervisor,
                            Arc::clone(&resource_service),
                        ));
                    let agent_service = Arc::new(agent::service::AgentService::new(
                        agent_repository,
                        production_agent_tools(),
                        production_agent_engine_capabilities(),
                    ));
                    if !app.manage(app_state::AppState::with_services(
                        service,
                        model_service,
                        resource_service,
                        agent_service,
                    )) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "PiWork application state is already managed",
                        )
                        .into());
                    }
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
            agent::commands::add_work_member
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
        for command in [
            "agent::commands::list_agent_instances",
            "agent::commands::list_capability_packs",
            "agent::commands::get_work_team",
            "agent::commands::validate_agent_assembly",
            "agent::commands::save_agent_copy",
            "agent::commands::add_work_member",
        ] {
            assert!(source.contains(command), "missing {command}");
        }
    }

    #[test]
    fn production_agent_allowlists_match_executable_b_stage_packs() {
        assert_eq!(
            super::production_agent_tools(),
            ["bash", "edit", "find", "grep", "ls", "read", "write"]
                .into_iter()
                .map(String::from)
                .collect()
        );
        assert!(super::production_agent_engine_capabilities().is_empty());
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
            .find("engine::supervisor::EngineSupervisor::new")
            .expect("publisher is not erased into the supervisor");

        assert!(observer < publisher && publisher < manage && manage < erase);
        assert!(assembly[publisher..manage].contains("observer.clone()"));
        let management = &assembly[manage..erase];
        assert!(management.contains("std::io::ErrorKind::AlreadyExists"));
        assert!(management.contains("PiWork activity observer is already managed"));
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
        assert!(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../NOTICE")
                .is_file()
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
