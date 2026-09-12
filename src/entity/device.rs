//! 推送设备实体（iOS APNs / Android 本地推送标识）。

use sea_orm::entity::prelude::*;

/// 设备模型（token 全局唯一，重新登录时更新归属）。
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "push_devices")]
pub struct Model {
    /// 设备记录 ID。
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// 归属用户。
    pub user_id: Uuid,
    /// 平台：ios / android。
    pub platform: String,
    /// 推送 token（APNs device token；Android 为客户端标识）。
    pub token: String,
    /// 设备名称（便于用户识别）。
    #[sea_orm(nullable)]
    pub device_name: Option<String>,
    /// 最近上报时间。
    pub last_seen_at: DateTimeWithTimeZone,
    /// 创建时间。
    pub created_at: DateTimeWithTimeZone,
    /// 解绑时间（退出登录）。
    #[sea_orm(nullable)]
    pub revoked_at: Option<DateTimeWithTimeZone>,
}

/// 关系定义。
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
