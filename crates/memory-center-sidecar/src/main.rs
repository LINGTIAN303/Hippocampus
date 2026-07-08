//! # OpenCode 压缩事件监听 sidecar（v2.36 新增）
//!
//! 监听 OpenCode SQLite 会话库的压缩事件，自动触发 MemoryCenter 归档。
//!
//! ## 架构
//!
//! ```text
//! ┌─────────────────┐      ┌──────────────────┐      ┌─────────────────┐
//! │   OpenCode      │      │  mc-sidecar      │      │  MemoryCenter   │
//! │                 │      │                  │      │                 │
//! │  session.db     │◄────│  SQLite 轮询     │      │  HTTP Server    │
//! │  (WAL mode)     │      │  (5s interval)   │      │                 │
//! │                 │      │                  │      │                 │
//! │  time_compacting│      │  检测压缩完成    │      │  /pre-compress  │
//! │  NULL→ts→NULL   │────►│  → 读 turns      │────►│  归档 + 摘要     │
//! │                 │      │  → 序列化        │      │                 │
//! └─────────────────┘      └──────────────────┘      └─────────────────┘
//! ```
//!
//! ## 使用方式
//!
//! ```bash
//! # 1. 启动 MemoryCenter HTTP 服务
//! mc-server
//!
//! # 2. 启动 sidecar（默认 5 秒轮询）
//! mc-sidecar --memorycenter-url http://127.0.0.1:8080
//!
//! # 3. backfill 模式（归档历史压缩会话）
//! mc-sidecar --backfill
//! ```
//!
//! ## 手动/自动压缩覆盖
//!
//! OpenCode 的压缩事件无论触发方式（自动 `compactIfNeeded` 或手动 `/compact`），
//! 最终都走 `compactAfterOverflow` → `Compaction.Started` → `Compaction.Ended` 流程，
//! `time_compacting` 字段都会经历 NULL → ts → NULL 变化。
//! 因此 sidecar 能覆盖两种压缩场景。

mod config;
mod opencode_db;
mod archive;
mod watcher;

use clap::Parser;
use config::SidecarConfig;
use opencode_db::OpenCodeDb;
use archive::ArchiveClient;
use watcher::CompactionWatcher;

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mc_sidecar=info".into()),
        )
        .init();

    let config = SidecarConfig::parse();

    // 解析 OpenCode SQLite 路径
    let db_path = match config.resolve_db_path() {
        Ok(path) => path,
        Err(e) => {
            tracing::error!(error = %e, "无法解析 OpenCode SQLite 路径，请通过 --opencode-db 指定");
            std::process::exit(1);
        }
    };

    tracing::info!(
        db_path = %db_path.display(),
        memorycenter_url = %config.memorycenter_url,
        poll_interval_secs = config.poll_interval,
        project_id = %config.project_id,
        backfill = config.backfill,
        "mc-sidecar 启动"
    );

    // 检查 db 文件是否存在
    if !db_path.exists() {
        tracing::error!(db_path = %db_path.display(), "OpenCode SQLite 文件不存在");
        tracing::error!("请确认 OpenCode 已安装并至少运行过一次");
        std::process::exit(1);
    }

    // 打开数据库
    let db = match OpenCodeDb::open(&db_path) {
        Ok(db) => db,
        Err(e) => {
            tracing::error!(error = %e, "打开 OpenCode SQLite 失败");
            std::process::exit(1);
        }
    };

    // 创建归档客户端
    let archive_client = ArchiveClient::new(&config);

    // 健康检查
    let healthy = archive_client.health_check().await.unwrap_or(false);
    if !healthy {
        tracing::warn!(
            url = %config.memorycenter_url,
            "MemoryCenter 服务不可达，sidecar 将继续运行并在检测到压缩时重试"
        );
    } else {
        tracing::info!(url = %config.memorycenter_url, "MemoryCenter 服务连接正常");
    }

    // 创建压缩事件检测器
    let mut watcher = CompactionWatcher::new();

    // backfill 模式：归档历史压缩会话
    if config.backfill {
        tracing::info!("backfill 模式：扫描历史压缩会话...");
        match watcher.backfill_sessions(&db) {
            Ok(sessions) => {
                for session in sessions {
                    archive_session(&db, &archive_client, &session, &config).await;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "backfill 扫描失败");
            }
        }
        tracing::info!("backfill 完成，进入持续监听模式");
    } else {
        // 首次轮询：建立基线状态（不触发归档）
        tracing::info!("首次轮询：建立基线状态...");
        if let Err(e) = watcher.poll(&db) {
            tracing::warn!(error = %e, "首次轮询失败");
        }
        tracing::info!("基线状态已建立，进入持续监听模式");
    }

    // 主循环
    let poll_interval = std::time::Duration::from_secs(config.poll_interval);
    loop {
        tokio::time::sleep(poll_interval).await;

        // 轮询检测压缩事件
        let poll_result = match watcher.poll(&db) {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!(error = %e, "轮询失败，等待下次重试");
                continue;
            }
        };

        if poll_result.compacting_count > 0 {
            tracing::debug!(
                compacting = poll_result.compacting_count,
                total = poll_result.total_sessions,
                "检测到正在压缩中的 session"
            );
        }

        // 处理新完成压缩的 session
        for session in poll_result.newly_compacted {
            archive_session(&db, &archive_client, &session, &config).await;
        }
    }
}

/// 归档单个 session
///
/// 1. 从 OpenCode SQLite 读取 session 的完整消息
/// 2. 序列化为 full_context 字符串
/// 3. 调用 MemoryCenter pre-compress 端点归档
async fn archive_session(
    db: &OpenCodeDb,
    archive_client: &ArchiveClient,
    session: &opencode_db::SessionInfo,
    config: &SidecarConfig,
) {
    tracing::info!(
        session_id = %session.id,
        session_title = %session.title,
        "开始归档压缩会话"
    );

    // 读取 session 消息
    let full_context = match db.read_session_context(&session.id, config.max_turns) {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::error!(
                session_id = %session.id,
                error = %e,
                "读取 session 消息失败"
            );
            return;
        }
    };

    if full_context.trim().is_empty() {
        tracing::warn!(
            session_id = %session.id,
            "session 消息为空，跳过归档"
        );
        return;
    }

    // 估算 token 数（字符数 / 3，与 MemoryCenter 默认估算一致）
    let estimated_tokens = full_context.len() / 3;

    tracing::info!(
        session_id = %session.id,
        context_chars = full_context.len(),
        estimated_tokens,
        "读取完成，调用 MemoryCenter pre-compress"
    );

    // 调用 MemoryCenter 归档
    match archive_client
        .pre_compress(&session.id, full_context, estimated_tokens, &config.project_id)
        .await
    {
        Ok(resp) => {
            tracing::info!(
                session_id = %session.id,
                hook_id = %resp.hook_id,
                parse_success = resp.parse_success,
                parsed_turns = resp.parsed_turns_count,
                archived_tokens = resp.archived_tokens,
                threshold = resp.threshold,
                ratio_percent = resp.threshold_ratio_percent,
                suggestion = %resp.suggestion,
                "✅ 归档成功"
            );
        }
        Err(e) => {
            tracing::error!(
                session_id = %session.id,
                error = %e,
                "❌ 归档失败"
            );
        }
    }
}
