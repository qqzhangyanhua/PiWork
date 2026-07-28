use std::{future::Future, sync::Arc};

use tauri::Manager;

pub mod app_state;
pub mod domain;
pub mod engine;
pub mod error;
pub mod paths;
pub mod storage;
pub mod work;

type StartupError = Box<dyn std::error::Error>;
type StartupResult<T> = Result<T, StartupError>;

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

async fn orchestrate_startup<T, E, Recover, RecoverFuture, Assemble, Show>(
    recover: Recover,
    assemble: Assemble,
    show: Show,
) -> Result<(), E>
where
    Recover: FnOnce() -> RecoverFuture,
    RecoverFuture: Future<Output = Result<T, E>>,
    Assemble: FnOnce(T) -> Result<(), E>,
    Show: FnOnce() -> Result<(), E>,
{
    let recovered = recover().await?;
    assemble(recovered)?;
    show()
}

fn application_builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = focus_visible_main_window(&window);
            }
        }))
        .setup(|app| {
            let paths = paths::AppPaths::from_resolver(app.path())?;
            // Tauri's setup callback runs synchronously on the event-loop thread. Bridge the
            // complete startup sequence once. The configured main window remains hidden until
            // migrations, recovery, and managed state assembly all succeed.
            let database_path = paths.database_path().to_path_buf();
            tauri::async_runtime::block_on(orchestrate_startup(
                || async move {
                    let database = storage::sqlite::Database::open(database_path).await?;
                    let repository = work::repository::WorkRepository::new(database.pool().clone());
                    work::service::WorkService::new(repository.clone())
                        .recover_interrupted_runs()
                        .await?;
                    Ok::<_, StartupError>(repository)
                },
                |repository| -> StartupResult<()> {
                    let engine = Arc::new(engine::fake::FakeEngineAdapter::new(
                        std::time::Duration::from_millis(120),
                    ));
                    let publisher = Arc::new(engine::publisher::TauriEventPublisher::new(
                        app.handle().clone(),
                    ));
                    let supervisor = Arc::new(engine::supervisor::EngineSupervisor::new(
                        repository.clone(),
                        engine,
                        publisher,
                        "Fake model",
                    ));
                    let service = Arc::new(work::service::WorkService::with_supervisor(
                        repository, supervisor,
                    ));
                    if !app.manage(app_state::AppState::new(service)) {
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
            ))
        })
        .invoke_handler(tauri::generate_handler![
            work::commands::create_work,
            work::commands::list_works,
            work::commands::get_work,
            work::commands::start_work
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
    async fn recovery_error_prevents_state_assembly_and_window_show() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recover_calls = Arc::clone(&calls);
        let assemble_calls = Arc::clone(&calls);
        let show_calls = Arc::clone(&calls);

        let result = super::orchestrate_startup(
            || async move {
                recover_calls.lock().unwrap().push("recover");
                Err::<(), _>("recovery failed")
            },
            |_| {
                assemble_calls.lock().unwrap().push("assemble");
                Ok(())
            },
            || {
                show_calls.lock().unwrap().push("show");
                Ok(())
            },
        )
        .await;

        assert_eq!(result, Err("recovery failed"));
        assert_eq!(*calls.lock().unwrap(), vec!["recover"]);
    }

    #[tokio::test]
    async fn successful_startup_shows_only_after_recovery_and_state_assembly() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recover_calls = Arc::clone(&calls);
        let assemble_calls = Arc::clone(&calls);
        let show_calls = Arc::clone(&calls);

        let result = super::orchestrate_startup(
            || async move {
                recover_calls.lock().unwrap().push("recover");
                Ok::<_, &str>("repository")
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
        )
        .await;

        assert_eq!(result, Ok(()));
        assert_eq!(*calls.lock().unwrap(), vec!["recover", "assemble", "show"]);
    }

    #[tokio::test]
    async fn state_assembly_error_prevents_window_show() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recover_calls = Arc::clone(&calls);
        let assemble_calls = Arc::clone(&calls);
        let show_calls = Arc::clone(&calls);

        let result = super::orchestrate_startup(
            || async move {
                recover_calls.lock().unwrap().push("recover");
                Ok::<_, &str>(())
            },
            |_| {
                assemble_calls.lock().unwrap().push("assemble");
                Err("assembly failed")
            },
            || {
                show_calls.lock().unwrap().push("show");
                Ok(())
            },
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
            || async move {
                recover_calls.lock().unwrap().push("recover");
                Ok::<_, &str>(())
            },
            |_| {
                assemble_calls.lock().unwrap().push("assemble");
                Ok(())
            },
            || {
                show_calls.lock().unwrap().push("show");
                Err("show failed")
            },
        )
        .await;

        assert_eq!(result, Err("show failed"));
        assert_eq!(*calls.lock().unwrap(), vec!["recover", "assemble", "show"]);
    }
}
