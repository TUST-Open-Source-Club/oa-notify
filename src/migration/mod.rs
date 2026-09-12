//! notify schema 迁移。

use sea_orm_migration::prelude::*;

/// 初始化迁移。
pub mod m20260913_000001_init;

/// 迁移入口。
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20260913_000001_init::Migration)]
    }
}
