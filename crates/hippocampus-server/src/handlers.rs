//! # API 端点处理器
//!
//! 5 个核心端点的 handler 实现。

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::AppError;
use crate::AppState;
use hippocampus_core::archive::Archiver;
use hippocampus_core::compact::Compactor;
use hippocampus_core::model::{ArchiveConfig, MemoryFile, MessageTurn};
use hippocampus_core::retrieve::{Retriever, SummaryView};
use hippocampus_core::score::DefaultScorer;
use hippocampus_core::storage::{LocalStorage, Storage};

// ============================================================================
// 请求 / 响应结构
// ============================================================================

/// archive 请求体
#[derive(Deserialize)]
pub struct ArchiveRequest {
    /// 待归档的轮次列表
    pub turns: Vec<MessageTurn>,
    /// 项目 ID（可选，影响存储路径）
    pub project_id: Option<String>,
}

/// compaction 请求体
#[derive(Deserialize)]
pub struct CompactionRequest {
    /// 周期类型："weekly" 或 "monthly"
    pub period: String,
    /// 项目 ID（可选）
    pub project_id: Option<String>,
}

/// compaction 响应（精简结构，与 FFI 层一致）
#[derive(Serialize)]
pub struct CompactionResult {
    pub memory_file_id: String,
    pub total_turns: usize,
    pub total_tokens: usize,
    pub hooks_count: usize,
    pub period: String,
}

/// prompt 响应
#[derive(Serialize)]
pub struct PromptResponse {
    pub prompt: String,
}

/// project_id 查询参数（GET 请求用）
#[derive(Deserialize)]
pub struct ProjectQuery {
    pub project_id: Option<String>,
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 创建 Storage 实例（每次请求创建，无内存缓存）
fn create_storage(state: &AppState) -> Arc<dyn Storage> {
    Arc::new(LocalStorage::new(state.storage_root.clone()))
}

// ============================================================================
// 5 个端点 handler
// ============================================================================

/// POST /api/v1/sessions/{sid}/archive
///
/// 归档一批轮次为记忆文件，生成索引钩子。
pub async fn archive(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Json(req): Json<ArchiveRequest>,
) -> Result<Json<SummaryView>, AppError> {
    if req.turns.is_empty() {
        return Err(AppError::BadRequest("turns 不能为空".to_string()));
    }

    let storage = create_storage(&state);
    let config = ArchiveConfig::default();
    let mut archiver = Archiver::new(config, storage, &sid, req.project_id);

    for turn in req.turns {
        archiver.push_turn(turn);
    }

    let (_, hook) = archiver.archive().await?;
    let summary = SummaryView::from(&hook);

    tracing::info!(
        session = %sid,
        hook_id = %summary.hook_id,
        tokens = summary.token_count,
        "归档成功"
    );

    Ok(Json(summary))
}

/// GET /api/v1/sessions/{sid}/memories/{hook_id}
///
/// 按钩子 ID 检索完整记忆文件。
pub async fn retrieve(
    State(state): State<AppState>,
    Path((sid, hook_id)): Path<(String, String)>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<MemoryFile>, AppError> {
    let storage = create_storage(&state);
    let retriever = Retriever::new(storage, &sid, query.project_id);

    let memory = retriever.retrieve_memory(&hook_id).await?;

    tracing::info!(
        session = %sid,
        hook_id = %hook_id,
        turns = memory.turns.len(),
        "检索成功"
    );

    Ok(Json(memory))
}

/// GET /api/v1/sessions/{sid}/summaries
///
/// 获取所有周期的摘要视图列表。
pub async fn get_summaries(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<Vec<SummaryView>>, AppError> {
    let storage = create_storage(&state);
    let retriever = Retriever::new(storage, &sid, query.project_id);

    let summaries = retriever.get_summaries().await?;

    tracing::info!(
        session = %sid,
        count = summaries.len(),
        "获取摘要成功"
    );

    Ok(Json(summaries))
}

/// GET /api/v1/sessions/{sid}/prompt
///
/// 渲染摘要为 system prompt 文本。
pub async fn render_prompt(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<PromptResponse>, AppError> {
    let storage = create_storage(&state);
    let retriever = Retriever::new(storage, &sid, query.project_id);

    let prompt = retriever.render_to_system_prompt().await?;

    tracing::info!(
        session = %sid,
        prompt_len = prompt.len(),
        "渲染 prompt 成功"
    );

    Ok(Json(PromptResponse { prompt }))
}

/// POST /api/v1/sessions/{sid}/compaction
///
/// 触发周期任务（周级合并 / 月级评分淘汰）。
pub async fn run_compaction(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Json(req): Json<CompactionRequest>,
) -> Result<Json<CompactionResult>, AppError> {
    let storage = create_storage(&state);
    let compactor = Compactor::new(
        storage,
        Box::new(DefaultScorer::new()),
        &sid,
        req.project_id,
    );

    let (memory, index_doc) = match req.period.as_str() {
        "weekly" => compactor.weekly_merge().await?,
        "monthly" => compactor.monthly_evict().await?,
        other => {
            return Err(AppError::BadRequest(format!(
                "无效的 period 值: {}（支持: weekly, monthly）",
                other
            )));
        }
    };

    let result = CompactionResult {
        memory_file_id: memory.id.to_string(),
        total_turns: memory.turns.len(),
        total_tokens: memory.total_tokens,
        hooks_count: index_doc.hooks.len(),
        period: req.period,
    };

    tracing::info!(
        session = %sid,
        period = %result.period,
        turns = result.total_turns,
        "周期任务完成"
    );

    Ok(Json(result))
}
