//! 用户通知偏好实体。

use sea_orm::entity::prelude::*;

/// 偏好模型（每用户一条）。
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "notification_preferences")]
pub struct Model {
    /// 记录 ID。
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// 用户 ID（唯一）。
    pub user_id: Uuid,
    /// 静音模块列表（JSON 数组，如 `["im"]`）。
    #[sea_orm(column_type = "JsonBinary")]
    pub muted_modules: Json,
    /// 免打扰开始时间（本地时间 `HH:MM`，空 = 不启用）。
    #[sea_orm(nullable)]
    pub quiet_from: Option<String>,
    /// 免打扰结束时间（本地时间 `HH:MM`）。
    #[sea_orm(nullable)]
    pub quiet_to: Option<String>,
    /// 是否接收 ntfy 实时推送。
    pub ntfy_enabled: bool,
    /// 更新时间。
    pub updated_at: DateTimeWithTimeZone,
}

/// 关系定义。
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
