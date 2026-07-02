//! # Hippocampus HTTP 服务入口
//!
//! 启动 Axum HTTP 服务，将 Core 的能力暴露为 REST API。

use hippocampus_server::{create_router, AppState, Config};
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hippocampus_server=info,tower_http=info".into()),
        )
        .init();

    let config = Config::default();

    // 确保存储目录存在
    std::fs::create_dir_all(&config.storage_root).expect("创建存储目录失败");

    let state = AppState {
        storage_root: config.storage_root.clone(),
    };

    let app = create_router(state).layer(TraceLayer::new_for_http());

    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Hippocampus HTTP 服务启动于 http://{}", addr);
    tracing::info!("存储根目录: {:?}", config.storage_root);
    tracing::info!("API 端点:");
    tracing::info!("  POST   /api/v1/sessions/{{sid}}/archive");
    tracing::info!("  GET    /api/v1/sessions/{{sid}}/memories/{{hook_id}}");
    tracing::info!("  GET    /api/v1/sessions/{{sid}}/summaries");
    tracing::info!("  GET    /api/v1/sessions/{{sid}}/prompt");
    tracing::info!("  POST   /api/v1/sessions/{{sid}}/compaction");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
