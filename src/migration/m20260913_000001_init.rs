//! 初始化 notify schema：notifications / notification_preferences / push_devices。

use sea_orm_migration::prelude::*;

/// 迁移定义。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("CREATE SCHEMA IF NOT EXISTS notify")
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Notifications::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Notifications::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Notifications::UserId).uuid().not_null())
                    .col(ColumnDef::new(Notifications::EventId).uuid().not_null())
                    .col(
                        ColumnDef::new(Notifications::EventType)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Notifications::Title)
                            .string_len(200)
                            .not_null(),
                    )
                    .col(ColumnDef::new(Notifications::Body).text().not_null())
                    .col(
                        ColumnDef::new(Notifications::Priority)
                            .string_len(16)
                            .not_null(),
                    )
                    .col(ColumnDef::new(Notifications::ResourceType).string_len(32))
                    .col(ColumnDef::new(Notifications::ResourceId).string_len(64))
                    .col(ColumnDef::new(Notifications::Url).string_len(512))
                    .col(ColumnDef::new(Notifications::ReadAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Notifications::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ux_notifications_user_event")
                    .table(Notifications::Table)
                    .col(Notifications::UserId)
                    .col(Notifications::EventId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ix_notifications_user_created")
                    .table(Notifications::Table)
                    .col(Notifications::UserId)
                    .col(Notifications::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(NotificationPreferences::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NotificationPreferences::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(NotificationPreferences::UserId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NotificationPreferences::MutedModules)
                            .json_binary()
                            .not_null(),
                    )
                    .col(ColumnDef::new(NotificationPreferences::QuietFrom).string_len(5))
                    .col(ColumnDef::new(NotificationPreferences::QuietTo).string_len(5))
                    .col(
                        ColumnDef::new(NotificationPreferences::NtfyEnabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(NotificationPreferences::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ux_notification_preferences_user")
                    .table(NotificationPreferences::Table)
                    .col(NotificationPreferences::UserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(PushDevices::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PushDevices::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(PushDevices::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(PushDevices::Platform)
                            .string_len(16)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PushDevices::Token)
                            .string_len(256)
                            .not_null(),
                    )
                    .col(ColumnDef::new(PushDevices::DeviceName).string_len(64))
                    .col(
                        ColumnDef::new(PushDevices::LastSeenAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PushDevices::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(PushDevices::RevokedAt).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ux_push_devices_token")
                    .table(PushDevices::Table)
                    .col(PushDevices::Token)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(PushDevices::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(NotificationPreferences::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Notifications::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

/// notifications 表标识符。
#[derive(DeriveIden)]
enum Notifications {
    Table,
    Id,
    UserId,
    EventId,
    EventType,
    Title,
    Body,
    Priority,
    ResourceType,
    ResourceId,
    Url,
    ReadAt,
    CreatedAt,
}

/// notification_preferences 表标识符。
#[derive(DeriveIden)]
enum NotificationPreferences {
    Table,
    Id,
    UserId,
    MutedModules,
    QuietFrom,
    QuietTo,
    NtfyEnabled,
    UpdatedAt,
}

/// push_devices 表标识符。
#[derive(DeriveIden)]
enum PushDevices {
    Table,
    Id,
    UserId,
    Platform,
    Token,
    DeviceName,
    LastSeenAt,
    CreatedAt,
    RevokedAt,
}
