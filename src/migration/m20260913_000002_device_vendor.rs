//! push_devices 支持多厂商：新增 vendor 列并按 (vendor, token) 唯一。

use sea_orm_migration::prelude::*;

/// 迁移定义。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PushDevices::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(PushDevices::Vendor)
                            .string_len(16)
                            .not_null()
                            .default("unknown"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("ux_push_devices_token")
                    .table(PushDevices::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ux_push_devices_vendor_token")
                    .table(PushDevices::Table)
                    .col(PushDevices::Vendor)
                    .col(PushDevices::Token)
                    .unique()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("ux_push_devices_vendor_token")
                    .table(PushDevices::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(PushDevices::Table)
                    .drop_column(PushDevices::Vendor)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

/// push_devices 表标识符。
#[derive(DeriveIden)]
enum PushDevices {
    /// 表。
    Table,
    /// vendor。
    Vendor,
    /// token。
    Token,
}
