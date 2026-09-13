//! 集成测试脚手架：真实 PostgreSQL + 独立 schema + FakeNtfy。

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use sea_orm_migration::MigratorTrait;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

use club_common::AppError;
use notify_service::config::Config;
use notify_service::migration::Migrator;
use notify_service::ntfy::{NtfyMessage, NtfyPublisher};
use notify_service::push::{PushChannel, PushMessage, PushRouter};
use notify_service::state::{AppState, SharedState};
use notify_service::{build_router, db};

/// 测试数据库连接串。
pub fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:55432/club_oa".to_string())
}

/// 记录 ntfy 推送的替身。
#[derive(Default)]
pub struct FakeNtfy {
    /// 已发布消息。
    pub published: Mutex<Vec<NtfyMessage>>,
}

#[async_trait]
impl NtfyPublisher for FakeNtfy {
    async fn publish(&self, message: &NtfyMessage) -> Result<(), AppError> {
        self.published.lock().expect("lock").push(message.clone());
        Ok(())
    }
}

impl FakeNtfy {
    /// 已发布消息数。
    pub fn count(&self) -> usize {
        self.published.lock().expect("lock").len()
    }
}

/// 可控制失败并记录调用的推送通道替身。
pub struct FakeChannel {
    /// 通道名。
    pub name: &'static str,
    /// 是否模拟失败。
    pub fail: AtomicBool,
    /// 调用记录。
    pub calls: Mutex<Vec<String>>,
}

impl FakeChannel {
    /// 创建通道。
    pub fn new(name: &'static str, fail: bool) -> Self {
        Self {
            name,
            fail: AtomicBool::new(fail),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// 设置失败开关。
    pub fn set_fail(&self, fail: bool) {
        self.fail.store(fail, Ordering::SeqCst);
    }

    /// 调用次数。
    pub fn count(&self) -> usize {
        self.calls.lock().expect("lock").len()
    }
}

#[async_trait::async_trait]
impl PushChannel for FakeChannel {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn send(
        &self,
        _device: &notify_service::entity::device::Model,
        _message: &PushMessage,
    ) -> Result<(), AppError> {
        self.calls.lock().expect("lock").push(self.name.to_string());
        if self.fail.load(Ordering::SeqCst) {
            Err(AppError::internal("fake channel down"))
        } else {
            Ok(())
        }
    }
}

/// 测试应用。
pub struct TestApp {
    /// 应用状态。
    pub state: SharedState,
    /// HTTP 路由。
    pub app: Router,
    /// ntfy 替身。
    pub ntfy: Arc<FakeNtfy>,
    /// 厂商通道替身（huawei）。
    pub vendor: Arc<FakeChannel>,
    /// FCM 替身（默认失败，便于验证降级）。
    pub fcm: Arc<FakeChannel>,
    /// 测试 schema 名。
    pub schema: String,
    /// 服务密钥（签发测试 JWT 用）。
    pub signing: club_auth_sdk::Jwks,
    /// 签名私钥 PEM。
    pub private_pem: Vec<u8>,
    /// 公钥 PEM（用于计算签发 kid）。
    pub public_pem: Vec<u8>,
}

/// 启动测试应用。
pub async fn spawn() -> TestApp {
    let url = test_database_url();
    let schema = format!("test_{}", Uuid::now_v7().simple());
    let database = db::connect_with_schema(&url, &schema)
        .await
        .expect("连接测试数据库失败");
    Migrator::up(&database, None).await.expect("迁移失败");

    // 生成测试密钥并把公钥直接注入状态（跳过 JWKS 网络请求）
    let private = notify_keygen::generate();
    let decoding = club_auth_sdk::decoding_key_from_rsa_pem(&private.public_pem).expect("公钥");
    let public_pem = String::from_utf8(private.public_pem.clone()).expect("pem utf8");
    let jwk = {
        let public = notify_keygen::parse_public(&public_pem);
        club_auth_sdk::jwk_from_rsa_public_components(
            &club_auth_sdk::jwks::key_id_from_pem(&private.public_pem),
            &public.0,
            &public.1,
        )
    };
    let jwks = club_auth_sdk::Jwks { keys: vec![jwk] };

    let mut env: HashMap<String, String> = HashMap::new();
    env.insert("DATABASE_URL".to_string(), url);
    env.insert("AUTH_ISSUER".to_string(), "https://oa.test".to_string());
    env.insert(
        "INTERNAL_SERVICE_SECRET".to_string(),
        "test-secret".to_string(),
    );
    env.insert("DEV_MODE".to_string(), "true".to_string());
    let config = Config::from_map(&env).expect("配置");

    let ntfy = Arc::new(FakeNtfy::default());
    let vendor = Arc::new(FakeChannel::new("huawei", false));
    let fcm = Arc::new(FakeChannel::new("fcm", true));
    let mut vendors: HashMap<String, Arc<dyn PushChannel>> = HashMap::new();
    vendors.insert("huawei".to_string(), vendor.clone());
    let push = Arc::new(PushRouter {
        vendors,
        fcm: Some(fcm.clone()),
        ntfy: ntfy.clone(),
    });
    let state = SharedState::new(AppState {
        db: database,
        config,
        ntfy: ntfy.clone(),
        push,
        signing_key: RwLock::new(Some(decoding)),
    });
    let app = build_router(state.clone());
    TestApp {
        state,
        app,
        ntfy,
        vendor,
        fcm,
        schema,
        signing: jwks,
        private_pem: private.private_pem.clone(),
        public_pem: private.public_pem,
    }
}

/// 为指定用户签发 Access Token。
pub fn issue_token(app: &TestApp, user_id: Uuid) -> String {
    let claims = club_auth_sdk::Claims {
        sub: user_id.to_string(),
        name: "测试用户".into(),
        avatar: None,
        roles: vec!["member".into()],
        scopes: vec!["im".into()],
        guest: false,
        iss: "https://oa.test".into(),
        iat: chrono::Utc::now().timestamp(),
        exp: chrono::Utc::now().timestamp() + 3600,
        jti: Uuid::now_v7().to_string(),
    };
    let kid = club_auth_sdk::jwks::key_id_from_pem(&app.public_pem);
    club_auth_sdk::encode_access_token(&claims, &app.private_pem, &kid).expect("签发测试令牌")
}

/// HTTP 响应快照。
pub struct TestResponse {
    /// 状态码。
    pub status: StatusCode,
    /// JSON 响应体。
    pub body: Value,
}

impl TestResponse {
    /// 断言状态码并返回 JSON。
    pub fn expect(self, status: StatusCode) -> Value {
        assert_eq!(self.status, status, "响应体: {}", self.body);
        self.body
    }
}

/// 发送 JSON 请求（可选 Bearer 令牌）。
pub async fn request(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<&Value>,
) -> TestResponse {
    request_with_headers(app, method, uri, token, body, &[]).await
}

/// 发送带额外请求头的 JSON 请求。
pub async fn request_with_headers(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<&Value>,
    extra_headers: &[(&str, String)],
) -> TestResponse {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    for (name, value) in extra_headers {
        builder = builder.header(*name, value);
    }
    let bytes = body
        .map(|value| serde_json::to_vec(value).expect("序列化"))
        .unwrap_or_default();
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(bytes)).expect("请求"))
        .await
        .expect("执行请求");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("响应体")
        .to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    TestResponse { status, body }
}

/// 以服务身份 POST 事件。
pub async fn service_post(app: &TestApp, service: &str, uri: &str, body: &Value) -> TestResponse {
    let body_bytes = serde_json::to_vec(body).expect("序列化");
    let timestamp = app.state.now().timestamp();
    let signature = club_auth_sdk::sign_service_token(
        service,
        app.state.config.internal_secret.as_bytes(),
        timestamp,
        &body_bytes,
    );
    let headers = [
        ("x-service-name", service.to_string()),
        ("x-service-timestamp", timestamp.to_string()),
        ("x-service-signature", signature),
    ];
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json");
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .app
        .clone()
        .oneshot(builder.body(Body::from(body_bytes)).expect("请求"))
        .await
        .expect("执行请求");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("响应体")
        .to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    TestResponse { status, body }
}

/// 测试用密钥生成辅助（避免在测试里再引入 rsa 依赖的样板）。
mod notify_keygen {
    /// 测试密钥。
    pub struct TestKey {
        /// 私钥 PEM。
        pub private_pem: Vec<u8>,
        /// 公钥 PEM。
        pub public_pem: Vec<u8>,
    }

    /// 生成 RSA 密钥对。
    pub fn generate() -> TestKey {
        // 复用 auth-sdk 的测试能力：通过 rsa crate 生成
        let mut rng = rand_core::OsRng;
        let private = rsa::RsaPrivateKey::new(&mut rng, 2048).expect("keygen");
        use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
        let private_pem = private
            .to_pkcs8_pem(LineEnding::LF)
            .expect("private pem")
            .as_bytes()
            .to_vec();
        let public_pem = rsa::RsaPublicKey::from(&private)
            .to_public_key_pem(LineEnding::LF)
            .expect("public pem")
            .into_bytes();
        TestKey {
            private_pem,
            public_pem,
        }
    }

    /// 解析公钥并返回 (模数, 指数) 大端字节。
    pub fn parse_public(pem: &str) -> (Vec<u8>, Vec<u8>) {
        use rsa::pkcs8::DecodePublicKey;
        use rsa::traits::PublicKeyParts;
        let public = rsa::RsaPublicKey::from_public_key_pem(pem).expect("parse public");
        (public.n().to_bytes_be(), public.e().to_bytes_be())
    }
}
