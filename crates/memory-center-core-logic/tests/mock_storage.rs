//! MockStorage 测试辅助 - 纯内存 Storage 实现

use memory_center_core_logic::model::*;
use memory_center_core_logic::storage::{Storage, SessionMeta};
use memory_center_core_logic::{Error, Result};
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct MockStorage {
    memories: RwLock<HashMap<String, MemoryFile>>,
    // key 用 period.as_str() 而非 ArchivePeriod 本身，因 model.rs 中 ArchivePeriod 未 derive Hash
    indexes: RwLock<HashMap<(String, Option<String>, &'static str), IndexDocument>>,
    session_meta: RwLock<HashMap<String, SessionMeta>>,
    raw_contexts: RwLock<HashMap<(String, String), String>>,
}

impl MockStorage {
    pub fn new() -> Self {
        Self {
            memories: RwLock::new(HashMap::new()),
            indexes: RwLock::new(HashMap::new()),
            session_meta: RwLock::new(HashMap::new()),
            raw_contexts: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MockStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Storage for MockStorage {
    async fn write_memory(&self, file: &MemoryFile) -> Result<String> {
        let memory_id = format!("mock-{}", file.id);
        self.memories.write().await.insert(memory_id.clone(), file.clone());
        Ok(memory_id)
    }

    async fn read_memory(&self, memory_id: &str) -> Result<MemoryFile> {
        self.memories.read().await.get(memory_id).cloned()
            .ok_or_else(|| Error::Storage(format!("记忆文件不存在: {}", memory_id)))
    }

    async fn delete_memory(&self, memory_id: &str) -> Result<()> {
        self.memories.write().await.remove(memory_id)
            .ok_or_else(|| Error::Storage(format!("记忆文件不存在: {}", memory_id)))
            .map(|_| ())
    }

    async fn write_index(&self, doc: &IndexDocument) -> Result<String> {
        let key = (doc.session_id.clone(), doc.project_id.clone(), doc.period.as_str());
        self.indexes.write().await.insert(key.clone(), doc.clone());
        Ok(format!("index-{:?}", key))
    }

    async fn read_index(&self, session_id: &str, project_id: Option<&str>, period: ArchivePeriod) -> Result<Option<IndexDocument>> {
        let key = (session_id.to_string(), project_id.map(|s| s.to_string()), period.as_str());
        Ok(self.indexes.read().await.get(&key).cloned())
    }

    async fn append_hook(&self, session_id: &str, project_id: Option<&str>, period: ArchivePeriod, hook: IndexHook) -> Result<()> {
        let key = (session_id.to_string(), project_id.map(|s| s.to_string()), period.as_str());
        let mut indexes = self.indexes.write().await;
        let doc = indexes.entry(key).or_insert_with(|| IndexDocument::new(
            session_id.to_string(),
            project_id.map(|s| s.to_string()),
            period,
        ));
        doc.hooks.push(hook);
        Ok(())
    }

    async fn list_memories(&self, _session_id: &str, _project_id: Option<&str>, _period: ArchivePeriod) -> Result<Vec<String>> {
        Ok(self.memories.read().await.keys().cloned().collect())
    }

    async fn write_session_meta(&self, session_id: &str, meta: &SessionMeta) -> Result<()> {
        self.session_meta.write().await.insert(session_id.to_string(), meta.clone());
        Ok(())
    }

    async fn read_session_meta(&self, session_id: &str) -> Result<Option<SessionMeta>> {
        Ok(self.session_meta.read().await.get(session_id).cloned())
    }

    async fn write_raw_context(&self, session_id: &str, hook_id: &str, content: &str) -> Result<String> {
        let path = format!("sessions/{}/raw_contexts/{}.txt", session_id, hook_id);
        self.raw_contexts.write().await.insert((session_id.to_string(), hook_id.to_string()), content.to_string());
        Ok(path)
    }

    async fn read_raw_context(&self, session_id: &str, hook_id: &str) -> Result<String> {
        self.raw_contexts.read().await.get(&(session_id.to_string(), hook_id.to_string())).cloned()
            .ok_or_else(|| Error::Storage(format!("raw_context 不存在: {}/{}", session_id, hook_id)))
    }

    async fn delete_raw_context(&self, session_id: &str, hook_id: &str) -> Result<()> {
        self.raw_contexts.write().await.remove(&(session_id.to_string(), hook_id.to_string()));
        Ok(())
    }
}
