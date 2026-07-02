//! # Hippocampus Python 绑定
//!
//! 使用 PyO3 将 [`hippocampus_core`] 的能力暴露为 Python 原生扩展模块。
//!
//! ## 架构
//!
//! - **同步 API**：内部 tokio runtime block_on（与 FFI 层一致）
//! - **OOP 风格**：`Hippocampus` 类持有句柄，方法 archive/retrieve/summaries/prompt/compaction
//! - **dict 数据类型**：Python dict 作为消息轮次的输入输出格式（通过 JSON 中间转换）
//! - **上下文管理器**：支持 `with Hippocampus(...) as hp:` 用法
//!
//! ## 使用示例
//!
//! ```python
//! from hippocampus_python import Hippocampus
//!
//! with Hippocampus("./data", "session-1", project_id="proj-a") as hp:
//!     # 归档
//!     summary = hp.archive([
//!         {"user_message": {"text": "你好"}, "llm_message": {"text": "你好！"}, ...}
//!     ])
//!     # 检索
//!     memory = hp.retrieve(summary["hook_id"])
//!     # 摘要列表
//!     summaries = hp.summaries()
//! ```

use hippocampus_core::archive::Archiver;
use hippocampus_core::compact::Compactor;
use hippocampus_core::model::ArchiveConfig;
use hippocampus_core::retrieve::Retriever;
use hippocampus_core::score::DefaultScorer;
use hippocampus_core::storage::{LocalStorage, Storage};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;

// ============================================================================
// Python 模块声明
// ============================================================================

/// Hippocampus Python 扩展模块
///
/// 模块名 `hippocampus_python`（与 Cargo.toml lib.name 一致）
#[pymodule]
mod hippocampus_python {
    use super::*;

    /// 模块级函数：返回版本号
    #[pyfunction]
    fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    /// 模块级函数：返回支持的操作列表
    #[pyfunction]
    fn operations() -> Vec<&'static str> {
        vec!["archive", "retrieve", "summaries", "prompt", "compaction"]
    }

    // 导出 Hippocampus 类
    #[pymodule_export]
    use super::Hippocampus;
}

// ============================================================================
// Hippocampus 类
// ============================================================================

/// Hippocampus 记忆库句柄
///
/// 持有存储根目录、tokio runtime、会话 ID 和项目 ID，
/// 一个实例对应一个会话（与 FFI 层 HippocampusHandle 一致）。
///
/// Python 用法：
/// ```python
/// hp = Hippocampus("./data", "session-1", project_id="proj-a")
/// summary = hp.archive(turns)
/// hp.close()  # 或用 with 上下文管理器
/// ```
#[pyclass(name = "Hippocampus")]
struct Hippocampus {
    /// 存储根目录
    storage_root: PathBuf,
    /// tokio 异步运行时（内部 block_on Core 异步方法）
    runtime: Runtime,
    /// 会话 ID
    session_id: String,
    /// 项目 ID（可选）
    project_id: Option<String>,
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 将 Python 对象转为 JSON 字符串
///
/// 使用 Python 内置 json 模块的 dumps 方法
fn py_to_json_string(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<String> {
    let json_mod = py.import("json")?;
    let dumps = json_mod.getattr("dumps")?;
    let result = dumps.call1((obj,))?;
    let s: String = result.extract()?;
    Ok(s)
}

/// 将 JSON 字符串转为 Python 对象
///
/// 使用 Python 内置 json 模块的 loads 方法
fn json_string_to_py<'py>(
    py: Python<'py>,
    json_str: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let json_mod = py.import("json")?;
    let loads = json_mod.getattr("loads")?;
    loads.call1((json_str,))
}

/// 创建 Storage 实例
fn create_storage(root: &std::path::Path) -> Arc<dyn Storage> {
    Arc::new(LocalStorage::new(root.to_path_buf()))
}

// ============================================================================
// Hippocampus 方法实现
// ============================================================================

#[pymethods]
impl Hippocampus {
    /// 创建新的 Hippocampus 句柄
    ///
    /// 参数：
    /// - `storage_root`：存储根目录路径
    /// - `session_id`：会话 ID
    /// - `project_id`：项目 ID（可选，默认 None）
    ///
    /// 返回：Hippocampus 实例
    #[new]
    #[pyo3(signature = (storage_root, session_id, project_id=None))]
    fn new(
        storage_root: String,
        session_id: String,
        project_id: Option<String>,
    ) -> PyResult<Self> {
        let root = PathBuf::from(&storage_root);
        // 确保存储目录存在
        std::fs::create_dir_all(&root).map_err(|e| {
            PyValueError::new_err(format!("创建存储目录失败 {}: {}", storage_root, e))
        })?;
        let runtime = Runtime::new().map_err(|e| {
            PyValueError::new_err(format!("创建 tokio runtime 失败: {}", e))
        })?;
        Ok(Self {
            storage_root: root,
            runtime,
            session_id,
            project_id,
        })
    }

    /// 上下文管理器：进入
    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    /// 上下文管理器：退出（自动释放 runtime）
    fn __exit__(
        &mut self,
        _exc_type: &Bound<'_, PyAny>,
        _exc_value: &Bound<'_, PyAny>,
        _traceback: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        // runtime 会在 drop 时自动释放，无需特殊处理
        Ok(false) // 不抑制异常
    }

    /// 友好的字符串表示
    fn __repr__(&self) -> String {
        format!(
            "Hippocampus(storage_root={:?}, session_id={:?}, project_id={:?})",
            self.storage_root, self.session_id, self.project_id
        )
    }

    /// 归档一批轮次为记忆文件
    ///
    /// 参数：
    /// - `turns`：消息轮次列表（list[dict]，每个 dict 符合 MessageTurn 结构）
    ///
    /// 返回：摘要视图 dict（含 hook_id/memory_file_id/summary_title/tags/archived_at/period/token_count）
    ///
    /// turn 结构示例：
    /// ```python
    /// {
    ///     "id": "uuid-string",  # 可选，不传会自动生成
    ///     "user_message": {"text": "...", "attachments": [], "tool_calls": [], "thinking": null},
    ///     "llm_message": {"text": "...", "attachments": [], "tool_calls": [], "thinking": null},
    ///     "tags": [{"kind": "Text"}],  # 17 类标签
    ///     "timestamp": "2026-07-02T12:00:00Z",  # 可选
    ///     "token_count": 100
    /// }
    /// ```
    fn archive(&self, turns: Vec<Py<PyAny>>) -> PyResult<Py<PyAny>> {
        if turns.is_empty() {
            return Err(PyValueError::new_err("turns 不能为空"));
        }

        // 1. 将 Python dict 列表转为 JSON 字符串数组
        let json_str: String = Python::attach(|py| -> PyResult<String> {
            let json_strings: PyResult<Vec<String>> = turns
                .iter()
                .map(|t| py_to_json_string(py, t.bind(py)))
                .collect();
            let json_strings = json_strings?;
            // 拼接成 JSON 数组
            Ok(format!("[{}]", json_strings.join(",")))
        })?;

        // 2. 反序列化为 Vec<MessageTurn>
        let message_turns: Vec<hippocampus_core::model::MessageTurn> =
            serde_json::from_str(&json_str).map_err(|e| {
                PyValueError::new_err(format!("解析 turns 失败: {}", e))
            })?;

        // 3. 调用 Core archive
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

        let (_, hook) = self
            .runtime
            .block_on(async { archiver.archive().await })
            .map_err(|e| PyValueError::new_err(format!("归档失败: {}", e)))?;

        // 4. 将 SummaryView 转为 Python dict
        let summary = hippocampus_core::retrieve::SummaryView::from(&hook);
        let summary_json = serde_json::to_string(&summary)
            .map_err(|e| PyValueError::new_err(format!("序列化摘要失败: {}", e)))?;

        Python::attach(|py| json_string_to_py(py, &summary_json).map(|b| b.into()))
    }

    /// 按钩子 ID 检索完整记忆文件
    ///
    /// 参数：
    /// - `hook_id`：钩子 ID（字符串）
    ///
    /// 返回：完整记忆文件 dict（含 turns 列表、session_id、project_id 等）
    fn retrieve(&self, hook_id: String) -> PyResult<Py<PyAny>> {
        let storage = create_storage(&self.storage_root);
        let retriever = Retriever::new(storage, &self.session_id, self.project_id.clone());

        let memory = self
            .runtime
            .block_on(async { retriever.retrieve_memory(&hook_id).await })
            .map_err(|e| PyValueError::new_err(format!("检索失败: {}", e)))?;

        let memory_json = serde_json::to_string(&memory)
            .map_err(|e| PyValueError::new_err(format!("序列化记忆失败: {}", e)))?;

        Python::attach(|py| json_string_to_py(py, &memory_json).map(|b| b.into()))
    }

    /// 获取所有周期的摘要视图列表
    ///
    /// 返回：摘要视图列表 list[dict]
    fn summaries(&self) -> PyResult<Vec<Py<PyAny>>> {
        let storage = create_storage(&self.storage_root);
        let retriever = Retriever::new(storage, &self.session_id, self.project_id.clone());

        let summaries = self
            .runtime
            .block_on(async { retriever.get_summaries().await })
            .map_err(|e| PyValueError::new_err(format!("获取摘要失败: {}", e)))?;

        let summaries_json = serde_json::to_string(&summaries)
            .map_err(|e| PyValueError::new_err(format!("序列化摘要失败: {}", e)))?;

        Python::attach(|py| {
            let arr = json_string_to_py(py, &summaries_json)?;
            // 转为 Vec<Py<PyAny>>
            let list: Bound<'_, pyo3::types::PyList> = arr.extract()?;
            list.iter().map(|item| Ok(item.into())).collect()
        })
    }

    /// 渲染摘要为 system prompt 文本
    ///
    /// 返回：prompt 字符串（可直接注入 system prompt）
    fn prompt(&self) -> PyResult<String> {
        let storage = create_storage(&self.storage_root);
        let retriever = Retriever::new(storage, &self.session_id, self.project_id.clone());

        let prompt = self
            .runtime
            .block_on(async { retriever.render_to_system_prompt().await })
            .map_err(|e| PyValueError::new_err(format!("渲染 prompt 失败: {}", e)))?;

        Ok(prompt)
    }

    /// 触发周期任务（周级合并 / 月级评分淘汰）
    ///
    /// 参数：
    /// - `period`：周期类型字符串 "weekly" 或 "monthly"
    ///
    /// 返回：精简结果 dict（memory_file_id/total_turns/total_tokens/hooks_count/period）
    fn compaction(&self, period: String) -> PyResult<Py<PyAny>> {
        let storage = create_storage(&self.storage_root);
        let compactor = Compactor::new(
            storage,
            Box::new(DefaultScorer::new()),
            &self.session_id,
            self.project_id.clone(),
        );

        let (memory, index_doc) = self
            .runtime
            .block_on(async {
                match period.as_str() {
                    "weekly" => compactor.weekly_merge().await,
                    "monthly" => compactor.monthly_evict().await,
                    other => Err(hippocampus_core::Error::Storage(format!(
                        "无效的 period 值: {}（支持: weekly, monthly）",
                        other
                    ))),
                }
            })
            .map_err(|e| PyValueError::new_err(format!("周期任务失败: {}", e)))?;

        // 构造结果（与 HTTP API 一致的精简结构）
        let result = serde_json::json!({
            "memory_file_id": memory.id.to_string(),
            "total_turns": memory.turns.len(),
            "total_tokens": memory.total_tokens,
            "hooks_count": index_doc.hooks.len(),
            "period": period,
        });
        let result_json = result.to_string();

        Python::attach(|py| json_string_to_py(py, &result_json).map(|b| b.into()))
    }

    /// 显式关闭（释放 runtime）
    ///
    /// 使用 with 上下文管理器时可自动调用
    fn close(&mut self) {
        // runtime 会在 drop 时自动释放，这里无需特殊处理
        // 保留方法供显式调用（API 兼容性）
    }
}
