//! MemoryStorage - 纯内存 Storage 实现
//!
//! 所有数据进程内存储，重启丢失。
//! 用于 demo / 测试 / 无状态计算 / 其他实现的 fallback。
//!
//! ## memory_id 格式
//!
//! 简化版：`memory-{uuid}`（不含路径分隔符，便于 JS 端处理）。
//! 与 LocalStorage 的路径格式 `sessions/{sid}/{period}/{uuid}.json` 不同。

use hippocampus_core_logic::model::{
    ArchivePeriod, IndexDocument, IndexHook, MemoryFile, MemoryUpdate,
};
use hippocampus_core_logic::storage::{SessionMeta, Storage};
use hippocampus_core_logic::{Error, Result};
use std::collections::HashMap;
use tokio::sync::RwLock;
use wasm_bindgen::prelude::*;

/// 纯内存 Storage 实现
///
/// 所有数据存储在进程内 HashMap 中，重启即丢失。
/// 适用于 WASM 环境（浏览器/Edge）下的 demo / 测试 / 无状态计算。
///
/// ## 设计
///
/// - `memory_id` 格式：`memory-{uuid}`（简化版，非路径格式）
/// - `list_memories` 按 `MemoryFile.session_id` + `period` 过滤
/// - `delete_index` 实际删除索引文档
/// - `update_access_count` 实际自增 access_count
/// - `raw_context` 按 `(session_id, hook_id)` 存储
#[wasm_bindgen]
pub struct MemoryStorage {
    /// memory_id → MemoryFile
    memories: RwLock<HashMap<String, MemoryFile>>,
    /// (session_id, project_id, period_str) → IndexDocument
    /// period 用 `as_str()` 返回的 `&'static str`，避免 ArchivePeriod 未派生 Hash
    indexes: RwLock<HashMap<(String, Option<String>, &'static str), IndexDocument>>,
    /// session_id → SessionMeta
    session_meta: RwLock<HashMap<String, SessionMeta>>,
    /// (session_id, hook_id) → content
    raw_contexts: RwLock<HashMap<(String, String), String>>,
}

#[wasm_bindgen]
impl MemoryStorage {
    /// 创建空的 MemoryStorage
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            memories: RwLock::new(HashMap::new()),
            indexes: RwLock::new(HashMap::new()),
            session_meta: RwLock::new(HashMap::new()),
            raw_contexts: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

// 与 Storage trait 一致：WASM 目标用 ?Send（单线程），native 目标用默认 Send
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl Storage for MemoryStorage {
    async fn write_memory(&self, file: &MemoryFile) -> Result<String> {
        // 简化版 memory_id：memory-{uuid}
        let memory_id = format!("memory-{}", file.id);
        self.memories
            .write()
            .await
            .insert(memory_id.clone(), file.clone());
        Ok(memory_id)
    }

    async fn read_memory(&self, memory_id: &str) -> Result<MemoryFile> {
        self.memories
            .read()
            .await
            .get(memory_id)
            .cloned()
            .ok_or_else(|| Error::Storage(format!("记忆文件不存在: {}", memory_id)))
    }

    async fn delete_memory(&self, memory_id: &str) -> Result<()> {
        self.memories
            .write()
            .await
            .remove(memory_id)
            .ok_or_else(|| Error::Storage(format!("记忆文件不存在: {}", memory_id)))
            .map(|_| ())
    }

    async fn write_index(&self, doc: &IndexDocument) -> Result<String> {
        let key = (
            doc.session_id.clone(),
            doc.project_id.clone(),
            doc.period.as_str(),
        );
        self.indexes.write().await.insert(key.clone(), doc.clone());
        Ok(format!(
            "index-{}/{}/{:?}",
            doc.session_id,
            doc.project_id.as_deref().unwrap_or(""),
            key.2
        ))
    }

    async fn read_index(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
    ) -> Result<Option<IndexDocument>> {
        let key = (
            session_id.to_string(),
            project_id.map(|s| s.to_string()),
            period.as_str(),
        );
        Ok(self.indexes.read().await.get(&key).cloned())
    }

    async fn delete_index(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
    ) -> Result<()> {
        let key = (
            session_id.to_string(),
            project_id.map(|s| s.to_string()),
            period.as_str(),
        );
        self.indexes.write().await.remove(&key);
        Ok(())
    }

    async fn append_hook(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
        hook: IndexHook,
    ) -> Result<()> {
        let key = (
            session_id.to_string(),
            project_id.map(|s| s.to_string()),
            period.as_str(),
        );
        let mut indexes = self.indexes.write().await;
        let doc = indexes.entry(key).or_insert_with(|| {
            IndexDocument::new(
                session_id.to_string(),
                project_id.map(|s| s.to_string()),
                period,
            )
        });
        doc.hooks.push(hook);
        Ok(())
    }

    async fn list_memories(
        &self,
        session_id: &str,
        _project_id: Option<&str>,
        period: ArchivePeriod,
    ) -> Result<Vec<String>> {
        // 按 session_id + period 过滤 MemoryFile
        let period_str = period.as_str();
        Ok(self
            .memories
            .read()
            .await
            .iter()
            .filter(|(_, file)| file.session_id == session_id && file.period.as_str() == period_str)
            .map(|(id, _)| id.clone())
            .collect())
    }

    async fn update_access_count(&self, memory_id: &str) -> Result<()> {
        let mut memories = self.memories.write().await;
        if let Some(file) = memories.get_mut(memory_id) {
            file.access_count += 1;
        }
        Ok(())
    }

    async fn update_memory(&self, memory_id: &str, updates: MemoryUpdate) -> Result<()> {
        let mut memories = self.memories.write().await;
        let file = memories
            .get_mut(memory_id)
            .ok_or_else(|| Error::Storage(format!("记忆文件不存在: {}", memory_id)))?;

        // 追加一条更新记录到 updates 历史
        let record = hippocampus_core_logic::model::MemoryUpdateRecord {
            updated_at: chrono::Utc::now(),
            update: updates,
            conflicts: Vec::new(),
        };
        file.updates.push(record);
        Ok(())
    }

    async fn write_session_meta(&self, session_id: &str, meta: &SessionMeta) -> Result<()> {
        self.session_meta
            .write()
            .await
            .insert(session_id.to_string(), meta.clone());
        Ok(())
    }

    async fn read_session_meta(&self, session_id: &str) -> Result<Option<SessionMeta>> {
        Ok(self
            .session_meta
            .read()
            .await
            .get(session_id)
            .cloned())
    }

    async fn write_raw_context(
        &self,
        session_id: &str,
        hook_id: &str,
        content: &str,
    ) -> Result<String> {
        let path = format!("sessions/{}/raw_contexts/{}.txt", session_id, hook_id);
        self.raw_contexts.write().await.insert(
            (session_id.to_string(), hook_id.to_string()),
            content.to_string(),
        );
        Ok(path)
    }

    async fn read_raw_context(&self, session_id: &str, hook_id: &str) -> Result<String> {
        self.raw_contexts
            .read()
            .await
            .get(&(session_id.to_string(), hook_id.to_string()))
            .cloned()
            .ok_or_else(|| {
                Error::Storage(format!("raw_context 不存在: {}/{}", session_id, hook_id))
            })
    }

    async fn delete_raw_context(&self, session_id: &str, hook_id: &str) -> Result<()> {
        // NotFound 视为成功（幂等，与 delete_index 行为一致）
        self.raw_contexts
            .write()
            .await
            .remove(&(session_id.to_string(), hook_id.to_string()));
        Ok(())
    }
}
