//! notify 服务入口：配置 → 数据库与迁移 → JWKS → ntfy 发布器 → HTTP 服务。

use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::Context;
use sea_orm_migration::MigratorTrait;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use notify_service::config::Config;
use notify_service::migration::Migrator;
use notify_service::ntfy::{HttpNtfyPublisher, LogNtfyPublisher};
use notify_service::state::{AppState, SharedState};
use notify_service::{build_router, db};

/// 初始化日志。
fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}

/// 启动前加载 JWKS（auth 可能稍后启动，重试一段时间）。
async fn load_jwks_with_retry(state: &AppState) -> anyhow::Result<()> {
    let mut last_error = None;
    for attempt in 1..=10 {
        match state.load_jwks().await {
            Ok(()) => {
                tracing::info!(attempt, "JWKS 加载成功");
                return Ok(());
            }
            Err(err) => {
                tracing::warn!(attempt, error = %err, "JWKS 加载失败，1 秒后重试");
                last_error = Some(err);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("JWKS 加载失败")))
}

/// 程序入口。
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let config = Config::from_env()?;
    tracing::info!(bind = %config.bind_addr, issuer = %config.issuer, "notify 服务启动中");

    let database = db::connect_with_schema(&config.database_url, "notify")
        .await
        .context("连接数据库失败")?;
    Migrator::up(&database, None)
        .await
        .context("执行数据库迁移失败")?;

    let publisher: Arc<dyn notify_service::ntfy::NtfyPublisher> = if config.dev_mode {
        Arc::new(LogNtfyPublisher)
    } else {
        Arc::new(HttpNtfyPublisher::new(
            &config.ntfy_base_url,
            &config.ntfy_token,
        ))
    };

    let state = SharedState::new(AppState {
        db: database,
        config,
        ntfy: publisher,
        signing_key: RwLock::new(None),
    });
    load_jwks_with_retry(&state).await?;

    // 配置了 Redis 时启用事件总线消费者
    if let Some(redis_url) = state.config.redis_url.clone() {
        let consumer_state = state.clone();
        let consumer_name = format!("notify-{}", std::process::id());
        tokio::spawn(async move {
            notify_service::consumer::run(consumer_state, redis_url, consumer_name).await;
        });
    } else {
        tracing::warn!("未配置 REDIS_URL，事件总线消费者未启动（仅支持内部 HTTP 投递）");
    }

    let listener = TcpListener::bind(&state.config.bind_addr)
        .await
        .with_context(|| format!("监听 {} 失败", state.config.bind_addr))?;
    tracing::info!(addr = %state.config.bind_addr, "HTTP 服务已就绪");
    axum::serve(listener, build_router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("收到退出信号，正在关闭");
        })
        .await
        .context("HTTP 服务异常退出")?;
    Ok(())
}
