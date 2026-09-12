//! 通知偏好接口。

use axum::extract::State;
use axum::Json;
use uuid::Uuid;

use club_auth_sdk::AuthUser;
use club_common::{AppError, FieldError};

use crate::domain::parse_hhmm;
use crate::dto::PreferenceDto;
use crate::repo;
use crate::state::SharedState;

/// 可静音的模块白名单。
const MUTABLE_MODULES: &[&str] = &["im", "task", "doc", "meeting", "event", "drive", "system"];

/// 解析登录用户 ID。
fn user_id_of(auth: &AuthUser) -> Result<Uuid, AppError> {
    auth.claims()
        .sub
        .parse()
        .map_err(|_| AppError::unauthorized("AUTH_INVALID_TOKEN", "访问令牌无效"))
}

/// `GET /preferences`：读取偏好（无记录时返回默认值）。
pub async fn get_preferences(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<Json<PreferenceDto>, AppError> {
    let user_id = user_id_of(&auth)?;
    let dto = repo::get_preference(&state.db, user_id)
        .await?
        .map(|model| PreferenceDto::from(&model))
        .unwrap_or_default();
    Ok(Json(dto))
}

/// `PUT /preferences`：覆盖保存偏好（校验静音模块与免打扰时间格式）。
pub async fn put_preferences(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(input): Json<PreferenceDto>,
) -> Result<Json<PreferenceDto>, AppError> {
    let user_id = user_id_of(&auth)?;
    let unknown: Vec<&String> = input
        .muted_modules
        .iter()
        .filter(|module| !MUTABLE_MODULES.contains(&module.as_str()))
        .collect();
    if !unknown.is_empty() {
        return Err(AppError::unprocessable(
            "NOTIFY_VALIDATION",
            "存在不支持的静音模块",
            vec![FieldError::new("mutedModules", "包含未知模块")],
        ));
    }
    for (field, value) in [
        ("quietFrom", input.quiet_from.as_deref()),
        ("quietTo", input.quiet_to.as_deref()),
    ] {
        if let Some(value) = value {
            if parse_hhmm(value).is_none() {
                return Err(AppError::unprocessable(
                    "NOTIFY_VALIDATION",
                    "免打扰时间格式应为 HH:MM",
                    vec![FieldError::new(field, "格式错误")],
                ));
            }
        }
    }
    let model = repo::upsert_preference(
        &state.db,
        user_id,
        input.muted_modules.clone(),
        input.quiet_from.clone(),
        input.quiet_to.clone(),
        input.ntfy_enabled,
        chrono::Utc::now(),
    )
    .await?;
    Ok(Json(PreferenceDto::from(&model)))
}
