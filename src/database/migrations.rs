use sqlx::migrate::Migrator;
use std::path::Path;

/// PostgreSQL 迁移器
pub static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

/// SQLite 迁移器
pub static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("./migrations/sqlite");

/// 检查迁移目录是否存在
pub fn check_migration_dirs() -> anyhow::Result<()> {
    let postgres_dir = Path::new("./migrations/postgres");
    let sqlite_dir = Path::new("./migrations/sqlite");
    
    if !postgres_dir.exists() {
        anyhow::bail!("PostgreSQL migration directory not found: {:?}", postgres_dir);
    }
    
    if !sqlite_dir.exists() {
        anyhow::bail!("SQLite migration directory not found: {:?}", sqlite_dir);
    }
    
    Ok(())
}