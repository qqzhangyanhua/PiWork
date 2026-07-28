use std::sync::Arc;

use tauri::Manager;

pub mod app_state;
pub mod domain;
pub mod engine;
pub mod error;
pub mod paths;
pub mod storage;
pub mod work;

fn application_builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .setup(|app| {
            let paths = paths::AppPaths::from_resolver(app.path())?;
            let database = tauri::async_runtime::block_on(storage::sqlite::Database::open(
                paths.database_path(),
            ))?;
            let repository = work::repository::WorkRepository::new(database.pool().clone());
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
            ));
            let service = Arc::new(work::service::WorkService::with_supervisor(
                repository, supervisor,
            ));
            app.manage(app_state::AppState::new(service));
            Ok(())
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
    #[test]
    fn application_builder_typechecks() {
        let _ = super::application_builder;
    }
}
