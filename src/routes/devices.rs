//! 推送设备接口（APNs token 注册 / 解绑 / 列表）。

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use club_auth_sdk::AuthUser;
use club_common::{AppError, FieldError};

use crate::dto::DeviceDto;
use crate::repo;
use crate::state::SharedState;

/// 允许的平台。
const PLATFORMS: &[&str] = &["ios", "android"];

/// 注册设备请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDeviceRequest {
    /// 平台：ios / android。
    pub platform: String,
    /// 推送 token。
    pub token: String,
    /// 设备名称。
    pub device_name: Option<String>,
}

/// 解析登录用户 ID。
fn user_id_of(auth: &AuthUser) -> Result<Uuid, AppError> {
    auth.claims()
        .sub
        .parse()
        .map_err(|_| AppError::unauthorized("AUTH_INVALID_TOKEN", "访问令牌无效"))
}

/// `POST /devices`：注册/更新设备。
pub async fn register_device(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(input): Json<RegisterDeviceRequest>,
) -> Result<Json<DeviceDto>, AppError> {
    let user_id = user_id_of(&auth)?;
    if !PLATFORMS.contains(&input.platform.as_str()) {
        return Err(AppError::unprocessable(
            "NOTIFY_VALIDATION",
            "平台不合法",
            vec![FieldError::new("platform", "仅支持 ios / android")],
        ));
    }
    if input.token.trim().len() < 8 {
        return Err(AppError::unprocessable(
            "NOTIFY_VALIDATION",
            "token 不合法",
            vec![FieldError::new("token", "token 过短")],
        ));
    }
    let model = repo::upsert_device(
        &state.db,
        user_id,
        &input.platform,
        input.token.trim(),
        input.device_name,
        chrono::Utc::now(),
    )
    .await?;
    Ok(Json(DeviceDto::from(&model)))
}

/// `DELETE /devices/{token}`：解绑设备（退出登录）。
pub async fn revoke_device(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(token): Path<String>,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    let revoked = repo::revoke_device(&state.db, user_id, &token, chrono::Utc::now()).await?;
    Ok(Json(json!({ "revoked": revoked })))
}

/// `GET /devices`：当前用户设备列表。
pub async fn list_devices(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<Json<Vec<DeviceDto>>, AppError> {
    let user_id = user_id_of(&auth)?;
    let devices = repo::list_devices(&state.db, user_id).await?;
    Ok(Json(devices.iter().map(DeviceDto::from).collect()))
}
