//! # Hippocampus Node.js 绑定（v2.14）
//!
//! 使用 napi-rs 3.x 将 [`hippocampus_core`] 的能力暴露为 Node.js 原生扩展模块。
//!
//! ## 架构
//!
//! - **异步 API**：所有 IO 方法返回 `Promise`（napi 的 `tokio_rt` feature 提供 runtime）
//! - **OOP 风格**：`Hippocampus` 类持有句柄，方法 archive/retrieve/summaries/prompt/compaction
//! - **JSON 中间转换**：JS 对象 ↔ JSON 字符串 ↔ Rust structs（与 Python 绑定一致）
//!
//! ## 使用示例
//!
//! ```javascript
//! const { Hippocampus } = require('./hippocampus-node');
//!
//! async function main() {
//!   const hp = new Hippocampus("./data", "session-1", "proj-a");
//!
//!   // 归档（turns 为 MessageTurn 数组）
//!   const summaryJson = await hp.archive(JSON.stringify(turns));
//!   const summary = JSON.parse(summaryJson);
//!
//!   // 检索
//!   const memoryJson = await hp.retrieve(summary.hook_id);
//!   const memory = JSON.parse(memoryJson);
//!
//!   // 摘要列表
//!   const summariesJson = await hp.summaries();
//!   const summaries = JSON.parse(summariesJson);
//!
//!   // 渲染 system prompt
//!   const prompt = await hp.prompt();
//!
//!   // 周期任务
//!   const resultJson = await hp.compaction("weekly");
//!   const result = JSON.parse(resultJson);
//!
//!   hp.close();
//! }
//! ```
//!
//! ## 与 Python 绑定的差异
//!
//! | 维度 | Python 绑定（PyO3） | Node.js 绑定（napi-rs） |
//! |------|--------------------|-----------------------|
//! | API 风格 | 同步（block_on） | 异步（Promise） |
//! | Runtime | 自持 tokio Runtime | napi 的 tokio_rt |
//! | 数据传递 | dict ↔ JSON ↔ struct | String(JSON) ↔ struct |
//! | 事件循环 | 阻塞 | 不阻塞 |

use hippocampus_core::archive::Archiver;
use hippocampus_core::compact::Compactor;
use hippocampus_core::model::ArchiveConfig;
use hippocampus_core::retrieve::{Retriever, SummaryView};
use hippocampus_core::score::DefaultScorer;
use hippocampus_core::storage::{LocalStorage, Storage};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::path::PathBuf;
use std::sync::Arc;

// ============================================================================
// 模块级函数
// ============================================================================

/// 返回版本号
#[napi]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 返回支持的操作列表
#[napi]
pub fn operations() -> Vec<&'static str> {
    vec!["archive", "retrieve", "summaries", "prompt", "compaction"]
}

// ============================================================================
// Hippocampus 类
// ============================================================================

/// Hippocampus 记忆库句柄
///
/// 持有存储根目录、会话 ID 和项目 ID，
/// 一个实例对应一个会话（与 Python 绑定一致）。
///
/// Node.js 用法：
/// ```javascript
/// const hp = new Hippocampus("./data", "session-1", "proj-a");
/// const summary = await hp.archive(JSON.stringify(turns));
/// hp.close();
/// ```
#[napi]
pub struct Hippocampus {
    /// 存储根目录
    storage_root: PathBuf,
    /// 会话 ID
    session_id: String,
    /// 项目 ID（可选）
    project_id: Option<String>,
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 创建 Storage 实例
fn create_storage(root: &std::path::Path) -> Arc<dyn Storage> {
    Arc::new(LocalStorage::new(root.to_path_buf()))
}

/// 将 Core Error 转为 napi Error
fn core_err_to_napi(e: hippocampus_core::Error) -> Error {
    Error::new(Status::GenericFailure, format!("{}", e))
}

/// 将 serde_json Error 转为 napi Error
fn serde_err_to_napi(e: serde_json::Error) -> Error {
    Error::new(Status::InvalidArg, format!("{}", e))
}

// ============================================================================
// Hippocampus 方法实现
// ============================================================================

#[napi]
impl Hippocampus {
    /// 创建新的 Hippocampus 句柄
    ///
    /// @param storageRoot - 存储根目录路径
    /// @param sessionId - 会话 ID
    /// @param projectId - 项目 ID（可选，默认 null）
    ///
    /// @returns Hippocampus 实例
    #[napi(constructor)]
    pub fn new(
        storage_root: String,
        session_id: String,
        project_id: Option<String>,
    ) -> Result<Self> {
        let root = PathBuf::from(&storage_root);
        // 确保存储目录存在
        std::fs::create_dir_all(&root).map_err(|e| {
            Error::new(
                Status::GenericFailure,
                format!("创建存储目录失败 {}: {}", storage_root, e),
            )
        })?;
        Ok(Self {
            storage_root: root,
            session_id,
            project_id,
        })
    }

    /// 归档一批轮次为记忆文件（异步）
    ///
    /// @param turnsJson - 消息轮次数组的 JSON 字符串
    ///
    /// @returns 摘要视图的 JSON 字符串（含 hook_id/memory_file_id/summary_title 等）
    ///
    /// turn 结构示例（JSON）：
    /// ```json
    /// {
    ///   "id": "uuid-string",
    ///   "user_message": {"text": "...", "attachments": [], "tool_calls": [], "thinking": null},
    ///   "llm_message": {"text": "...", "attachments": [], "tool_calls": [], "thinking": null},
    ///   "tags": [{"kind": "Text"}],
    ///   "timestamp": "2026-07-02T12:00:00Z",
    ///   "token_count": 100
    /// }
    /// ```
    #[napi]
    pub async fn archive(&self, turns_json: String) -> Result<String> {
        if turns_json.trim().is_empty() {
            return Err(Error::new(Status::InvalidArg, "turnsJson 不能为空"));
        }

        // 1. 解析 JSON 为 Vec<MessageTurn>
        let message_turns: Vec<hippocampus_core::model::MessageTurn> =
            serde_json::from_str(&turns_json).map_err(serde_err_to_napi)?;

        if message_turns.is_empty() {
            return Err(Error::new(Status::InvalidArg, "turns 不能为空"));
        }

        // 2. 调用 Core archive
        let storage = create_storage(&self.storage_root);
        let config = ArchiveConfig::default();
        let mut archiver = Archiver::new(
            config,
            storage,
            &self.session_id,
            self.project_id.clone(),
        );

        for turn in message_turns {
            archiver.push_turn(turn);
        }

        let (_, hook) = archiver.archive().await.map_err(core_err_to_napi)?;

        // 3. 将 SummaryView 转为 JSON 字符串
        let summary = SummaryView::from(&hook);
        serde_json::to_string(&summary).map_err(serde_err_to_napi)
    }

    /// 按钩子 ID 检索完整记忆文件（异步）
    ///
    /// @param hookId - 钩子 ID（字符串）
    ///
    /// @returns 完整记忆文件的 JSON 字符串（含 turns 列表、session_id 等）
    #[napi]
    pub async fn retrieve(&self, hook_id: String) -> Result<String> {
        let storage = create_storage(&self.storage_root);
        let retriever = Retriever::new(storage, &self.session_id, self.project_id.clone());

        let memory = retriever
            .retrieve_memory(&hook_id)
            .await
            .map_err(core_err_to_napi)?;

        serde_json::to_string(&memory).map_err(serde_err_to_napi)
    }

    /// 获取所有周期的摘要视图列表（异步）
    ///
    /// @returns 摘要视图列表的 JSON 字符串
    #[napi]
    pub async fn summaries(&self) -> Result<String> {
        let storage = create_storage(&self.storage_root);
        let retriever = Retriever::new(storage, &self.session_id, self.project_id.clone());

        let summaries = retriever
            .get_summaries()
            .await
            .map_err(core_err_to_napi)?;

        serde_json::to_string(&summaries).map_err(serde_err_to_napi)
    }

    /// 渲染摘要为 system prompt 文本（异步）
    ///
    /// @returns prompt 字符串（可直接注入 system prompt）
    #[napi]
    pub async fn prompt(&self) -> Result<String> {
        let storage = create_storage(&self.storage_root);
        let retriever = Retriever::new(storage, &self.session_id, self.project_id.clone());

        retriever
            .render_to_system_prompt()
            .await
            .map_err(core_err_to_napi)
    }

    /// 触发周期任务（周级合并 / 月级评分淘汰）（异步）
    ///
    /// @param period - 周期类型字符串 "weekly" 或 "monthly"
    ///
    /// @returns 精简结果的 JSON 字符串（memory_file_id/total_turns/total_tokens/hooks_count/period）
    #[napi]
    pub async fn compaction(&self, period: String) -> Result<String> {
        let storage = create_storage(&self.storage_root);
        let compactor = Compactor::new(
            storage,
            Box::new(DefaultScorer::new()),
            &self.session_id,
            self.project_id.clone(),
        );

        let (memory, index_doc) = match period.as_str() {
            "weekly" => compactor.weekly_merge().await,
            "monthly" => compactor.monthly_evict().await,
            other => Err(hippocampus_core::Error::Storage(format!(
                "无效的 period 值: {}（支持: weekly, monthly）",
                other
            ))),
        }
        .map_err(core_err_to_napi)?;

        // 构造结果（与 HTTP API / Python 绑定一致的精简结构）
        let result = serde_json::json!({
            "memory_file_id": memory.id.to_string(),
            "total_turns": memory.turns.len(),
            "total_tokens": memory.total_tokens,
            "hooks_count": index_doc.hooks.len(),
            "period": period,
        });

        serde_json::to_string(&result).map_err(serde_err_to_napi)
    }

    /// 显式关闭（释放资源）
    ///
    /// Node.js 没有 Python 的上下文管理器（with），调用 close() 可显式释放。
    /// 不调用也会在 GC 时自动释放。
    #[napi]
    pub fn close(&self) {
        // napi 异步模式下无需显式释放 runtime
        // 保留方法供 API 兼容性（与 Python 绑定一致）
    }

    /// 友好的字符串表示
    #[napi]
    pub fn to_string(&self) -> String {
        format!(
            "Hippocampus(storage_root={:?}, session_id={:?}, project_id={:?})",
            self.storage_root, self.session_id, self.project_id
        )
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_returns_cargo_version() {
        let v = version();
        assert!(!v.is_empty());
        assert_eq!(v, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_operations_returns_expected_list() {
        let ops = operations();
        assert_eq!(ops.len(), 5);
        assert!(ops.contains(&"archive"));
        assert!(ops.contains(&"retrieve"));
        assert!(ops.contains(&"summaries"));
        assert!(ops.contains(&"prompt"));
        assert!(ops.contains(&"compaction"));
    }

    #[test]
    fn test_hippocampus_new_creates_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("hippocampus_test");
        let hp = Hippocampus::new(
            root.to_str().unwrap().to_string(),
            "sess-test".to_string(),
            None,
        );
        assert!(hp.is_ok(), "创建 Hippocampus 应成功");
        assert!(root.exists(), "存储目录应被创建");
    }

    #[test]
    fn test_hippocampus_new_with_project_id() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("hippocampus_proj");
        let hp = Hippocampus::new(
            root.to_str().unwrap().to_string(),
            "sess-proj".to_string(),
            Some("project-a".to_string()),
        )
        .unwrap();
        assert_eq!(hp.session_id, "sess-proj");
        assert_eq!(hp.project_id, Some("project-a".to_string()));
    }

    #[test]
    fn test_hippocampus_to_string() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("hippocampus_repr");
        let hp = Hippocampus::new(
            root.to_str().unwrap().to_string(),
            "sess-repr".to_string(),
            Some("proj-x".to_string()),
        )
        .unwrap();
        let s = hp.to_string();
        assert!(s.contains("sess-repr"));
        assert!(s.contains("proj-x"));
    }

    #[test]
    fn test_hippocampus_close_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("hippocampus_close");
        let hp = Hippocampus::new(
            root.to_str().unwrap().to_string(),
            "sess-close".to_string(),
            None,
        )
        .unwrap();
        // close 应不 panic
        hp.close();
    }

    #[tokio::test]
    async fn test_hippocampus_archive_empty_json_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("hippocampus_empty");
        let hp = Hippocampus::new(
            root.to_str().unwrap().to_string(),
            "sess-empty".to_string(),
            None,
        )
        .unwrap();

        // 空字符串应返回错误
        let result = hp.archive("".to_string()).await;
        assert!(result.is_err(), "空 turnsJson 应返回错误");

        // 空数组也应返回错误
        let result = hp.archive("[]".to_string()).await;
        assert!(result.is_err(), "空 turns 数组应返回错误");
    }

    #[tokio::test]
    async fn test_hippocampus_archive_invalid_json_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("hippocampus_invalid");
        let hp = Hippocampus::new(
            root.to_str().unwrap().to_string(),
            "sess-invalid".to_string(),
            None,
        )
        .unwrap();

        // 无效 JSON 应返回错误
        let result = hp.archive("not a json".to_string()).await;
        assert!(result.is_err(), "无效 JSON 应返回错误");
    }

    #[tokio::test]
    async fn test_hippocampus_retrieve_nonexistent_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("hippocampus_retrieve");
        let hp = Hippocampus::new(
            root.to_str().unwrap().to_string(),
            "sess-retrieve".to_string(),
            None,
        )
        .unwrap();

        // 不存在的 hook_id 应返回错误
        let result = hp.retrieve("nonexistent-hook-id".to_string()).await;
        assert!(result.is_err(), "检索不存在的 hook_id 应返回错误");
    }

    #[tokio::test]
    async fn test_hippocampus_summaries_empty_returns_empty_array() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("hippocampus_summaries");
        let hp = Hippocampus::new(
            root.to_str().unwrap().to_string(),
            "sess-summaries".to_string(),
            None,
        )
        .unwrap();

        let result = hp.summaries().await;
        assert!(result.is_ok(), "空存储应返回空数组而非错误");
        let json = result.unwrap();
        assert_eq!(json, "[]", "空存储的摘要列表应为空数组 JSON");
    }

    #[tokio::test]
    async fn test_hippocampus_compaction_invalid_period_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("hippocampus_compaction");
        let hp = Hippocampus::new(
            root.to_str().unwrap().to_string(),
            "sess-compaction".to_string(),
            None,
        )
        .unwrap();

        // 无效 period 应返回错误
        let result = hp.compaction("daily".to_string()).await;
        assert!(result.is_err(), "无效 period 应返回错误");
    }

    #[tokio::test]
    async fn test_hippocampus_archive_full_workflow() {
        // 端到端测试：archive → retrieve → summaries → prompt
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("hippocampus_e2e");
        let hp = Hippocampus::new(
            root.to_str().unwrap().to_string(),
            "sess-e2e".to_string(),
            None,
        )
        .unwrap();

        // 1. 构造一个最小的 MessageTurn JSON
        //    id 必须是有效的 UUID（MessageTurn.id 字段为 Uuid 类型，不接受 null）
        let turn_json = serde_json::json!([{
            "id": "00000000-0000-0000-0000-000000000001",
            "user_message": {"text": "你好", "attachments": [], "tool_calls": [], "thinking": null},
            "llm_message": {"text": "你好！有什么可以帮你的？", "attachments": [], "tool_calls": [], "thinking": null},
            "tags": [{"kind": "Text"}],
            "timestamp": "2026-07-04T12:00:00Z",
            "token_count": 50
        }]);
        let turns_json = turn_json.to_string();

        // 2. archive
        let summary_json = hp.archive(turns_json).await.expect("归档应成功");
        let summary: serde_json::Value = serde_json::from_str(&summary_json).unwrap();
        assert!(summary["hook_id"].is_string(), "摘要应包含 hook_id");
        assert!(summary["memory_id"].is_string(), "摘要应包含 memory_id");
        assert!(summary["summary_title"].is_string());
        let hook_id = summary["hook_id"].as_str().unwrap().to_string();

        // 3. retrieve
        let memory_json = hp.retrieve(hook_id.clone()).await.expect("检索应成功");
        let memory: serde_json::Value = serde_json::from_str(&memory_json).unwrap();
        assert_eq!(memory["session_id"], "sess-e2e");
        assert!(memory["turns"].is_array());
        assert_eq!(memory["turns"].as_array().unwrap().len(), 1);

        // 4. summaries
        let summaries_json = hp.summaries().await.expect("摘要列表应成功");
        let summaries: serde_json::Value = serde_json::from_str(&summaries_json).unwrap();
        assert!(summaries.is_array());
        assert_eq!(summaries.as_array().unwrap().len(), 1);

        // 5. prompt
        let prompt = hp.prompt().await.expect("渲染 prompt 应成功");
        assert!(!prompt.is_empty(), "prompt 不应为空");
    }
}
