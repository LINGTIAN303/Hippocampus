//! # Hippocampus HTTP 服务库
//!
//! 将 [`hippocampus_core`] 的核心能力暴露为 REST API，
//! 供所有语言（Python/Node/Go/Java 等）通过 HTTP 调用。
//!
//! ## 架构
//!
//! - **无状态设计**：每次请求从磁盘读取，操作完释放
//! - **Storage 共享**：`AppState` 持有存储根目录，每次请求创建 `LocalStorage`
//! - **Archiver 一次性模式**：客户端一次性传入 turns，服务端 push 后归档
//!
//! ## API 端点
//!
//! | 方法 | 路径 | 作用 |
//! |------|------|------|
//! | POST | `/api/v1/sessions/{sid}/archive` | 归档 turns |
//! | GET  | `/api/v1/sessions/{sid}/memories/{hook_id}` | 检索记忆 |
//! | GET  | `/api/v1/sessions/{sid}/summaries` | 摘要视图 |
//! | GET  | `/api/v1/sessions/{sid}/prompt` | 渲染 system prompt |
//! | POST | `/api/v1/sessions/{sid}/compaction` | 周期任务 |

mod error;
mod handlers;

pub use error::AppError;

use std::path::PathBuf;

/// 应用配置（从环境变量读取）
#[derive(Debug, Clone)]
pub struct Config {
    /// 监听地址
    pub host: String,
    /// 监听端口
    pub port: u16,
    /// 存储根目录
    pub storage_root: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: std::env::var("HIPPOCAMPUS_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: std::env::var("HIPPOCAMPUS_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8765),
            storage_root: PathBuf::from(
                std::env::var("HIPPOCAMPUS_ROOT").unwrap_or_else(|_| "./data".to_string()),
            ),
        }
    }
}

/// 应用共享状态（通过 Axum State 提取器注入）
#[derive(Clone)]
pub struct AppState {
    /// 存储根目录（每次请求创建 LocalStorage 时使用）
    pub storage_root: PathBuf,
}

/// 创建路由
pub fn create_router(state: AppState) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        // 5 个核心端点
        .route(
            "/api/v1/sessions/{sid}/archive",
            post(handlers::archive),
        )
        .route(
            "/api/v1/sessions/{sid}/memories/{hook_id}",
            get(handlers::retrieve),
        )
        .route(
            "/api/v1/sessions/{sid}/summaries",
            get(handlers::get_summaries),
        )
        .route(
            "/api/v1/sessions/{sid}/prompt",
            get(handlers::render_prompt),
        )
        .route(
            "/api/v1/sessions/{sid}/compaction",
            post(handlers::run_compaction),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8765);
        assert_eq!(config.storage_root, PathBuf::from("./data"));
    }
}
