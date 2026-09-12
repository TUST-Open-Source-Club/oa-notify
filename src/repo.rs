//! 数据访问层：只操作 notify schema。

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TryInsertResult,
};
use serde_json::Value;
use uuid::Uuid;

use club_common::{new_id, AppError, CursorParams};

use crate::entity::{device, notification, preference};

/// 将数据库错误映射为统一错误。
pub fn map_db_err(err: DbErr) -> AppError {
    AppError::internal(err)
}

/// 新建通知的输入。
#[derive(Debug, Clone)]
pub struct NewNotification {
    /// 接收用户。
    pub user_id: Uuid,
    /// 来源事件 ID（幂等键）。
    pub event_id: Uuid,
    /// 事件类型。
    pub event_type: String,
    /// 标题。
    pub title: String,
    /// 摘要正文。
    pub body: String,
    /// 优先级。
    pub priority: String,
    /// 资源类型。
    pub resource_type: Option<String>,
    /// 资源 ID。
    pub resource_id: Option<String>,
    /// 跳转路径。
    pub url: Option<String>,
}

/// 幂等写入通知：重复事件（同 user + event_id）直接忽略。
///
/// 返回 `true` 表示新建成功，`false` 表示已存在（重复投递）。
pub async fn insert_notification_idempotent(
    db: &DatabaseConnection,
    new: NewNotification,
    now: DateTime<Utc>,
) -> Result<bool, AppError> {
    let model = notification::ActiveModel {
        id: Set(new_id()),
        user_id: Set(new.user_id),
        event_id: Set(new.event_id),
        event_type: Set(new.event_type),
        title: Set(new.title),
        body: Set(new.body),
        priority: Set(new.priority),
        resource_type: Set(new.resource_type),
        resource_id: Set(new.resource_id),
        url: Set(new.url),
        read_at: Set(None),
        created_at: Set(now.fixed_offset()),
    };
    // SeaORM 2.0：冲突时忽略，并用 TryInsertResult 区分「已插入 / 已存在」
    let result = notification::Entity::insert(model)
        .on_conflict_do_nothing_on([notification::Column::UserId, notification::Column::EventId])
        .exec_without_returning(db)
        .await
        .map_err(map_db_err)?;
    Ok(matches!(result, TryInsertResult::Inserted(_)))
}

/// 游标分页查询通知（按创建时间倒序），`unread_only` 仅返回未读。
///
/// 返回的记录可能比 limit 多一条，供调用方裁剪并生成 nextCursor。
pub async fn list_notifications(
    db: &DatabaseConnection,
    user_id: Uuid,
    unread_only: bool,
    params: &CursorParams,
) -> Result<Vec<notification::Model>, AppError> {
    let limit = i64::from(params.limit()) + 1;
    let mut select = notification::Entity::find()
        .filter(notification::Column::UserId.eq(user_id))
        .order_by_desc(notification::Column::CreatedAt)
        .order_by_desc(notification::Column::Id)
        .limit(limit as u64);
    if unread_only {
        select = select.filter(notification::Column::ReadAt.is_null());
    }
    select.all(db).await.map_err(map_db_err)
}

/// 未读数量。
pub async fn unread_count(db: &DatabaseConnection, user_id: Uuid) -> Result<i64, AppError> {
    notification::Entity::find()
        .filter(notification::Column::UserId.eq(user_id))
        .filter(notification::Column::ReadAt.is_null())
        .count(db)
        .await
        .map(|count| count as i64)
        .map_err(map_db_err)
}

/// 将指定通知标记为已读（仅限本人）。
pub async fn mark_read(
    db: &DatabaseConnection,
    user_id: Uuid,
    ids: &[Uuid],
    now: DateTime<Utc>,
) -> Result<u64, AppError> {
    if ids.is_empty() {
        return Ok(0);
    }
    let result = notification::Entity::update_many()
        .col_expr(
            notification::Column::ReadAt,
            sea_orm::sea_query::Expr::value(now.fixed_offset()),
        )
        .filter(notification::Column::UserId.eq(user_id))
        .filter(notification::Column::Id.is_in(ids.to_vec()))
        .filter(notification::Column::ReadAt.is_null())
        .exec(db)
        .await
        .map_err(map_db_err)?;
    Ok(result.rows_affected)
}

/// 全部标记已读。
pub async fn mark_all_read(
    db: &DatabaseConnection,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<u64, AppError> {
    let result = notification::Entity::update_many()
        .col_expr(
            notification::Column::ReadAt,
            sea_orm::sea_query::Expr::value(now.fixed_offset()),
        )
        .filter(notification::Column::UserId.eq(user_id))
        .filter(notification::Column::ReadAt.is_null())
        .exec(db)
        .await
        .map_err(map_db_err)?;
    Ok(result.rows_affected)
}

/// 读取偏好；不存在时返回 None（由调用方决定默认值）。
pub async fn get_preference(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Option<preference::Model>, AppError> {
    preference::Entity::find()
        .filter(preference::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(map_db_err)
}

/// 覆盖写入偏好（存在则更新）。
pub async fn upsert_preference(
    db: &DatabaseConnection,
    user_id: Uuid,
    muted_modules: Vec<String>,
    quiet_from: Option<String>,
    quiet_to: Option<String>,
    ntfy_enabled: bool,
    now: DateTime<Utc>,
) -> Result<preference::Model, AppError> {
    let muted = Value::Array(muted_modules.into_iter().map(Value::String).collect());
    if let Some(existing) = get_preference(db, user_id).await? {
        let mut active: preference::ActiveModel = existing.into();
        active.muted_modules = Set(muted);
        active.quiet_from = Set(quiet_from);
        active.quiet_to = Set(quiet_to);
        active.ntfy_enabled = Set(ntfy_enabled);
        active.updated_at = Set(now.fixed_offset());
        return active.update(db).await.map_err(map_db_err);
    }
    preference::ActiveModel {
        id: Set(new_id()),
        user_id: Set(user_id),
        muted_modules: Set(muted),
        quiet_from: Set(quiet_from),
        quiet_to: Set(quiet_to),
        ntfy_enabled: Set(ntfy_enabled),
        updated_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)
}

/// 注册/更新推送设备（token 唯一，重新登录时转移归属）。
pub async fn upsert_device(
    db: &DatabaseConnection,
    user_id: Uuid,
    platform: &str,
    token: &str,
    device_name: Option<String>,
    now: DateTime<Utc>,
) -> Result<device::Model, AppError> {
    if let Some(existing) = device::Entity::find()
        .filter(device::Column::Token.eq(token))
        .one(db)
        .await
        .map_err(map_db_err)?
    {
        let mut active: device::ActiveModel = existing.into();
        active.user_id = Set(user_id);
        active.platform = Set(platform.to_string());
        active.device_name = Set(device_name);
        active.last_seen_at = Set(now.fixed_offset());
        active.revoked_at = Set(None);
        return active.update(db).await.map_err(map_db_err);
    }
    device::ActiveModel {
        id: Set(new_id()),
        user_id: Set(user_id),
        platform: Set(platform.to_string()),
        token: Set(token.to_string()),
        device_name: Set(device_name),
        last_seen_at: Set(now.fixed_offset()),
        created_at: Set(now.fixed_offset()),
        revoked_at: Set(None),
    }
    .insert(db)
    .await
    .map_err(map_db_err)
}

/// 解绑推送设备（退出登录时调用）。
pub async fn revoke_device(
    db: &DatabaseConnection,
    user_id: Uuid,
    token: &str,
    now: DateTime<Utc>,
) -> Result<bool, AppError> {
    let result = device::Entity::update_many()
        .col_expr(
            device::Column::RevokedAt,
            sea_orm::sea_query::Expr::value(now.fixed_offset()),
        )
        .filter(device::Column::UserId.eq(user_id))
        .filter(device::Column::Token.eq(token))
        .filter(device::Column::RevokedAt.is_null())
        .exec(db)
        .await
        .map_err(map_db_err)?;
    Ok(result.rows_affected > 0)
}

/// 列出用户的活跃设备。
pub async fn list_devices(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<device::Model>, AppError> {
    device::Entity::find()
        .filter(device::Column::UserId.eq(user_id))
        .filter(device::Column::RevokedAt.is_null())
        .order_by_desc(device::Column::LastSeenAt)
        .all(db)
        .await
        .map_err(map_db_err)
}
