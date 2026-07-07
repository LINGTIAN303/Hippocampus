//! 测试辅助模块 - 纯内存 Storage 实现
//!
//! 模拟 LocalStorage 的关键行为，供 archive/compact/retrieve 单元测试使用：
//! - memory_id 格式：`sessions/{session_id}/{period}/{uuid}.json`（与 LocalStorage 一致）
//! - list_memories 按 session_id + period 过滤（支持 cleanup_daily 等测试）
//! - delete_index 实际删除索引（支持 IMP-02 cleanup 测试）
//! - update_access_count 实际自增（支持 IMP-01 retrieve 计数测试）

use crate::model::{ArchivePeriod, IndexDocument, IndexHook, MemoryFile};
use crate::storage::{SessionMeta, Storage};
use crate::{Error, Result};
use std::collections::HashMap;
use tokio::sync::RwLock;

/// 纯内存 Storage 实现（测试用）
///
/// 行为对齐 LocalStorage：
/// - `write_memory` 返回路径格式 `sessions/{session_id}/{period}/{uuid}.json`
/// - `list_memories` 按路径前缀过滤 session_id + period
/// - `delete_index` 实际删除索引文档
/// - `update_access_count` 实际自增 access_count
pub struct InMemoryStorage {
    /// memory_id → MemoryFile
    memories: RwLock<HashMap<String, MemoryFile>>,
    /// (session_id, project_id, period_str) → IndexDocument
    indexes: RwLock<HashMap<(String, Option<String>, &'static str), IndexDocument>>,
    /// session_id → SessionMeta
    session_meta: RwLock<HashMap<String, SessionMeta>>,
    /// (session_id, hook_id) → content
    raw_contexts: RwLock<HashMap<(String, String), String>>,
}

impl InMemoryStorage {
    /// 创建空的内存 Storage
    pub fn new() -> Self {
        Self {
            memories: RwLock::new(HashMap::new()),
            indexes: RwLock::new(HashMap::new()),
            session_meta: RwLock::new(HashMap::new()),
            raw_contexts: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Storage for InMemoryStorage {
    async fn write_memory(&self, file: &MemoryFile) -> Result<String> {
        // 模拟 LocalStorage 的 memory_id 格式：sessions/{session_id}/{period}/{id}.json
        let memory_id = format!(
            "sessions/{}/{}/{}.json",
            file.session_id,
            file.period.as_dir_name(),
            file.id
        );
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
        Ok(format!("index-{:?}", key))
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
        // 模拟 LocalStorage 的目录扫描：按 session_id + period 路径前缀过滤
        let prefix = format!("sessions/{}/{}/", session_id, period.as_dir_name());
        Ok(self
            .memories
            .read()
            .await
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect())
    }

    async fn update_access_count(&self, memory_id: &str) -> Result<()> {
        let mut memories = self.memories.write().await;
        if let Some(file) = memories.get_mut(memory_id) {
            file.access_count += 1;
        }
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
        self.raw_contexts
            .write()
            .await
            .remove(&(session_id.to_string(), hook_id.to_string()));
        Ok(())
    }
}
