//! 数据库连接辅助：确保 schema 存在并将 search_path 固定到 notify。

use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr};

/// 创建（若不存在）`schema` 并返回以该 schema 为 search_path 的连接池。
pub async fn connect_with_schema(url: &str, schema: &str) -> Result<DatabaseConnection, DbErr> {
    if schema.is_empty()
        || !schema
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(DbErr::Custom(format!("非法 schema 名称: {schema}")));
    }
    let bootstrap = Database::connect(url).await?;
    bootstrap
        .execute_unprepared(&format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\""))
        .await?;
    bootstrap.close().await?;

    let mut options = ConnectOptions::new(url.to_string());
    options.set_schema_search_path(schema);
    options.max_connections(10);
    options.acquire_timeout(Duration::from_secs(5));
    Database::connect(options).await
}
