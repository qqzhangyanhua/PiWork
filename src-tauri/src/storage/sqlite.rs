use std::{path::Path, time::Duration};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

use crate::error::AppError;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));

        Self::connect(options, None).await
    }

    pub async fn open_in_memory() -> Result<Self, AppError> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));

        Self::connect(options, Some(1)).await
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    #[doc(hidden)]
    pub async fn table_names(&self) -> Result<Vec<String>, AppError> {
        let names =
            sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
                .fetch_all(&self.pool)
                .await?;

        Ok(names)
    }

    async fn connect(
        options: SqliteConnectOptions,
        max_connections: Option<u32>,
    ) -> Result<Self, AppError> {
        let mut pool_options = SqlitePoolOptions::new();
        if let Some(max_connections) = max_connections {
            pool_options = pool_options.max_connections(max_connections);
        }

        let pool = pool_options.connect_with(options).await?;
        MIGRATOR.run(&pool).await?;

        Ok(Self { pool })
    }
}
