//! 环境变量配置。

use std::collections::HashMap;

use anyhow::anyhow;

/// notify 服务运行配置。
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL 连接串。
    pub database_url: String,
    /// HTTP 监听地址。
    pub bind_addr: String,
    /// JWT issuer（与 auth 一致）。
    pub issuer: String,
    /// 内部服务令牌共享密钥。
    pub internal_secret: String,
    /// ntfy 服务基地址（内网）。
    pub ntfy_base_url: String,
    /// ntfy 访问令牌（发布用）。
    pub ntfy_token: String,
    /// ntfy 对外基地址（供前端订阅）。
    pub ntfy_public_url: String,
    /// 开发模式：使用日志发布器而非真实 ntfy。
    pub dev_mode: bool,
}

impl Config {
    /// 从进程环境变量加载。
    pub fn from_env() -> anyhow::Result<Self> {
        let map: HashMap<String, String> = std::env::vars().collect();
        Self::from_map(&map)
    }

    /// 从键值映射加载（便于测试）。
    pub fn from_map(map: &HashMap<String, String>) -> anyhow::Result<Self> {
        let get = |key: &str| map.get(key).map(String::as_str);
        Ok(Self {
            database_url: get("DATABASE_URL")
                .ok_or_else(|| anyhow!("缺少必填环境变量 DATABASE_URL"))?
                .to_string(),
            bind_addr: get("NOTIFY_BIND_ADDR")
                .unwrap_or("0.0.0.0:8088")
                .to_string(),
            issuer: get("AUTH_ISSUER")
                .unwrap_or("http://localhost:8081")
                .trim_end_matches('/')
                .to_string(),
            internal_secret: get("INTERNAL_SERVICE_SECRET")
                .unwrap_or("dev-internal-secret")
                .to_string(),
            ntfy_base_url: get("NTFY_BASE_URL")
                .unwrap_or("http://localhost:8090")
                .trim_end_matches('/')
                .to_string(),
            ntfy_token: get("NTFY_TOKEN").unwrap_or_default().to_string(),
            ntfy_public_url: get("NTFY_PUBLIC_URL")
                .unwrap_or("http://localhost:8090")
                .trim_end_matches('/')
                .to_string(),
            dev_mode: matches!(get("DEV_MODE"), Some("1") | Some("true")),
        })
    }
}
