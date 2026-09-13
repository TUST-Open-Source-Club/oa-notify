//! 应用状态与 JWT 校验（从 auth 的 JWKS 拉取公钥并缓存）。

use std::sync::{Arc, RwLock};

use anyhow::Context;
use jsonwebtoken::DecodingKey;
use sea_orm::DatabaseConnection;

use club_auth_sdk::{decode_access_token, Claims, TokenVerifier};
use club_common::AppError;

use crate::config::Config;
use crate::ntfy::NtfyPublisher;

/// 应用状态。
pub struct AppState {
    /// 数据库连接（search_path=notify）。
    pub db: DatabaseConnection,
    /// 运行配置。
    pub config: Config,
    /// ntfy 发布端口。
    pub ntfy: Arc<dyn NtfyPublisher>,
    /// 多通道推送路由。
    pub push: Arc<crate::push::PushRouter>,
    /// 用于验签的解码 key（JWKS 缓存）。
    pub signing_key: RwLock<Option<DecodingKey>>,
}

impl AppState {
    /// 当前 UTC 时间（后续可抽为 Clock trait 以便测试注入）。
    pub fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }

    /// 从 auth 的 JWKS 端点加载公钥。
    pub async fn load_jwks(&self) -> anyhow::Result<()> {
        let url = format!("{}/.well-known/jwks.json", self.config.issuer);
        let jwks: club_auth_sdk::Jwks = reqwest::get(&url)
            .await
            .context("请求 JWKS 失败")?
            .error_for_status()
            .context("JWKS 返回错误状态")?
            .json()
            .await
            .context("解析 JWKS 失败")?;
        let key = club_auth_sdk::jwks::decoding_key_from_jwks(&jwks, None)
            .map_err(|err| anyhow::anyhow!("JWKS 无可用公钥: {err}"))?;
        *self.signing_key.write().expect("signing_key lock") = Some(key);
        Ok(())
    }
}

impl TokenVerifier for AppState {
    /// 使用缓存的 JWKS 公钥本地验签。
    fn verify_token(&self, token: &str) -> Result<Claims, AppError> {
        let guard = self.signing_key.read().expect("signing_key lock");
        let key = guard
            .as_ref()
            .ok_or_else(|| AppError::internal("JWKS 尚未加载"))?;
        decode_access_token(token, key, &self.config.issuer)
            .map_err(|_| AppError::unauthorized("AUTH_INVALID_TOKEN", "访问令牌无效或已过期"))
    }
}

/// 共享状态句柄（实现 TokenVerifier 供登录提取器使用）。
#[derive(Clone)]
pub struct SharedState(Arc<AppState>);

impl SharedState {
    /// 包装应用状态。
    pub fn new(state: AppState) -> Self {
        Self(Arc::new(state))
    }

    /// 内部引用。
    pub fn inner(&self) -> &AppState {
        &self.0
    }
}

impl std::ops::Deref for SharedState {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TokenVerifier for SharedState {
    /// 委托给 AppState。
    fn verify_token(&self, token: &str) -> Result<Claims, AppError> {
        self.0.verify_token(token)
    }
}
