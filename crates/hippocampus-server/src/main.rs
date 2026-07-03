//! # Hippocampus HTTP 服务入口
//!
//! 启动 Axum HTTP 服务，将 Core 的能力暴露为 REST API。
//!
//! ## 语义检索配置（v2.5 批次 7，v2.13 默认值更新）
//!
//! 通过环境变量配置 Embedder API 后，`/search` 端点和归档后自动索引生效：
//!
//! | 环境变量 | 说明 | 默认值 |
//! |---------|------|--------|
//! | `HIPPOCAMPUS_EMBEDDER_API_URL` | Embedding API 地址（OpenAI 兼容 `/v1/embeddings`） | 空（降级为仅关键词） |
//! | `HIPPOCAMPUS_EMBEDDER_API_KEY` | API Key | 空 |
//! | `HIPPOCAMPUS_EMBEDDER_MODEL` | 模型名 | `text-embedding-3-large` |
//! | `HIPPOCAMPUS_EMBEDDER_DIM` | 向量维度 | `3072` |
//! | `HIPPOCAMPUS_EMBEDDER_TIMEOUT` | 超时秒数 | `30` |
//!
//! 未配置 `API_URL` 时，自动降级为 `KeywordOnlyRetriever`（仅 BM25 关键词检索）。
//!
//! ## 冲突检测配置（v2.10，v2.13 默认值更新）
//!
//! | 环境变量 | 说明 | 默认值 |
//! |---------|------|--------|
//! | `HIPPOCAMPUS_DETECTOR_API_URL` | LLM API 地址（OpenAI 兼容 `/v1/chat/completions`） | 空（降级为 HeuristicDetector） |
//! | `HIPPOCAMPUS_DETECTOR_API_KEY` | API Key | 空 |
//! | `HIPPOCAMPUS_DETECTOR_MODEL` | 模型名 | `gpt-5.5-instant` |
//! | `HIPPOCAMPUS_DETECTOR_TIMEOUT` | 超时秒数 | `30` |
//! | `HIPPOCAMPUS_DETECTOR_MAX_TOKENS` | LLM 最大输出 token | `500` |
//!
//! 未配置 `API_URL` 时，使用 `HeuristicDetector`（启发式纯算法，无 LLM 依赖）。

use hippocampus_server::{create_router, AppState, Config};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

/// 从环境变量读取 Embedder 配置并构造 SessionSearchRouter
///
/// v2.8：替代 v2.5 的全局单例 build_search_components
/// v2.13：使用 `EmbedderConfig::from_env()` 简化环境变量读取
///
/// - 配置完整：每 session 独立 HybridRetriever（关键词 + 向量 + RRF 融合）
/// - 未配置或失败：每 session 独立 KeywordOnlyRetriever（仅关键词，降级模式）
fn build_session_search() -> Option<Arc<hippocampus_server::SessionSearchRouter>> {
    use hippocampus_core::semantic::Embedder;
    use hippocampus_server::{EmbedderConfig, HttpEmbedder, SessionSearchRouter};

    // v2.13：使用 EmbedderConfig::from_env() 统一环境变量读取
    let embedder_config = match EmbedderConfig::from_env() {
        Some(config) => config,
        None => {
            // 降级模式：仅关键词检索（每 session 独立）
            tracing::info!("未配置 Embedder API，降级为仅关键词检索（KeywordOnlyRetriever，session 级隔离）");
            return Some(Arc::new(SessionSearchRouter::new(None, 0)));
        }
    };

    let dim = embedder_config.dim;
    tracing::info!(
        api_url = %embedder_config.api_url,
        model = %embedder_config.model,
        dim = embedder_config.dim,
        "Embedder 已配置，启用 session 级混合检索（HybridRetriever）"
    );

    let embedder: Arc<dyn Embedder> = Arc::new(HttpEmbedder::new(embedder_config));
    Some(Arc::new(SessionSearchRouter::new(Some(embedder), dim)))
}

/// 从环境变量构造冲突检测器（v2.10，v2.13 简化）
///
/// - 配置了 `HIPPOCAMPUS_DETECTOR_API_URL` + `API_KEY`：返回 `HttpLlmDetector`（LLM 语义级检测）
/// - 未配置：返回 `HeuristicDetector`（启发式纯算法，无 LLM 依赖）
fn build_conflict_detector() -> std::sync::Arc<dyn hippocampus_core::conflict::ConflictDetector> {
    use hippocampus_server::{HttpLlmDetector, LlmDetectorConfig};

    // v2.13：使用 LlmDetectorConfig::from_env() 统一环境变量读取
    let config = match LlmDetectorConfig::from_env() {
        Some(config) => config,
        None => {
            tracing::info!(
                "冲突检测器：未配置 LLM API，使用 HeuristicDetector（启发式纯算法，三维度检测）"
            );
            return std::sync::Arc::new(hippocampus_core::heuristic::HeuristicDetector::new());
        }
    };

    tracing::info!(
        api_url = %config.api_url,
        model = %config.model,
        max_tokens = config.max_tokens,
        "冲突检测器：LLM API 已配置，使用 HttpLlmDetector（语义级冲突检测，失败时降级为空报告）"
    );
    std::sync::Arc::new(HttpLlmDetector::new(config))
}

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

    // v2.8：构造 Session 级索引隔离路由器（替代 v2.5 全局单例）
    let session_search = build_session_search();

    // v2.10：构造冲突检测器（环境变量驱动：LLM 优先，降级为 HeuristicDetector）
    let conflict_detector = Some(build_conflict_detector());

    let state = AppState {
        storage_root: config.storage_root.clone(),
        retriever: None,            // v2.8 起由 session_search 替代
        search_indexer: None,       // v2.8 起由 session_search 替代
        session_search,
        conflict_detector,
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
    tracing::info!("  POST   /api/v1/sessions/{{sid}}/search");
    tracing::info!("  GET    /api/v1/sessions/{{sid}}/memories/{{hook_id}}/conflicts");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
