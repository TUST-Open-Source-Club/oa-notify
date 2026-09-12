//! 站内通知实体。

use sea_orm::entity::prelude::*;

/// 通知模型（`(user_id, event_id)` 唯一，保证事件重复投递幂等）。
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "notifications")]
pub struct Model {
    /// 通知 ID。
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// 接收用户。
    pub user_id: Uuid,
    /// 来源事件 ID（幂等键）。
    pub event_id: Uuid,
    /// 事件类型，如 `task.assigned`。
    pub event_type: String,
    /// 标题。
    pub title: String,
    /// 正文摘要。
    #[sea_orm(column_type = "Text")]
    pub body: String,
    /// 优先级：urgent / high / default / low。
    pub priority: String,
    /// 资源类型。
    #[sea_orm(nullable)]
    pub resource_type: Option<String>,
    /// 资源 ID。
    #[sea_orm(nullable)]
    pub resource_id: Option<String>,
    /// 点击跳转路径。
    #[sea_orm(nullable)]
    pub url: Option<String>,
    /// 已读时间。
    #[sea_orm(nullable)]
    pub read_at: Option<DateTimeWithTimeZone>,
    /// 创建时间。
    pub created_at: DateTimeWithTimeZone,
}

/// 关系定义。
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
