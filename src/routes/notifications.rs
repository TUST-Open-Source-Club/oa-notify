//! 用户通知接口：列表 / 未读数 / 已读。

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use club_auth_sdk::AuthUser;
use club_common::{AppError, CursorPage, CursorParams};

use crate::dto::NotificationDto;
use crate::repo;
use crate::state::SharedState;

/// 列表查询参数（显式声明避免 serde_urlencoded 的 flatten 问题）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    /// 游标（上一页最后一条 ID）。
    pub cursor: Option<Uuid>,
    /// 每页条数。
    pub limit: Option<u32>,
    /// 仅未读。
    pub unread_only: Option<bool>,
}

impl ListQuery {
    /// 转换为游标分页参数。
    fn params(&self) -> CursorParams {
        CursorParams {
            cursor: None,
            limit: self.limit,
        }
    }
}

/// 解析登录用户 ID。
fn user_id_of(auth: &AuthUser) -> Result<Uuid, AppError> {
    auth.claims()
        .sub
        .parse()
        .map_err(|_| AppError::unauthorized("AUTH_INVALID_TOKEN", "访问令牌无效"))
}

/// `GET /notifications`：游标分页列表。
pub async fn list_notifications(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Json<CursorPage<NotificationDto>>, AppError> {
    let user_id = user_id_of(&auth)?;
    let params = query.params();
    let mut items = repo::list_notifications(
        &state.db,
        user_id,
        query.unread_only.unwrap_or(false),
        query.cursor,
        &params,
    )
    .await?;
    let limit = params.limit() as usize;
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items.last().map(|item| item.id.to_string())
    } else {
        None
    };
    Ok(Json(CursorPage::new(
        items.iter().map(NotificationDto::from).collect(),
        next_cursor,
    )))
}

/// `GET /notifications/unread-count`：未读数。
pub async fn unread_count(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    let count = repo::unread_count(&state.db, user_id).await?;
    Ok(Json(json!({ "count": count })))
}

/// `POST /notifications/{id}/read`：单条已读。
pub async fn mark_read(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    let affected = repo::mark_read(&state.db, user_id, &[id], chrono::Utc::now()).await?;
    Ok(Json(json!({ "updated": affected })))
}

/// `POST /notifications/read-all`：全部已读。
pub async fn read_all(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    let affected = repo::mark_all_read(&state.db, user_id, chrono::Utc::now()).await?;
    Ok(Json(json!({ "updated": affected })))
}
