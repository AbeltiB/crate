use anyhow::{Context, Result};
use r2d2_sqlite::SqliteConnectionManager;
use std::path::Path;

pub type DbPool = r2d2::Pool<SqliteConnectionManager>;

refinery::embed_migrations!("migrations");

pub fn init_pool(app_data_dir: &Path) -> Result<DbPool> {
    std::fs::create_dir_all(app_data_dir).context("creating app data dir")?;
    let db_path = app_data_dir.join("crate.db");

    let manager = SqliteConnectionManager::file(&db_path).with_init(|conn| {
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
    });
    let pool = r2d2::Pool::new(manager).context("creating sqlite connection pool")?;

    let mut conn = pool.get().context("getting connection for migrations")?;
    migrations::runner()
        .run(&mut *conn)
        .context("running database migrations")?;

    Ok(pool)
}
