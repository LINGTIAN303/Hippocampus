//! MockStorage 基础 CRUD 测试

mod mock_storage;  // 引入同目录的 mock_storage.rs

use hippocampus_core_logic::model::*;
use hippocampus_core_logic::storage::{Storage, SessionMeta};
use chrono::Utc;
use uuid::Uuid;

#[tokio::test]
async fn test_mock_storage_write_read_memory() {
    let storage = mock_storage::MockStorage::new();
    let file = MemoryFile {
        id: Uuid::new_v4(),
        schema_version: 1,
        archived_at: Utc::now(),
        session_id: "test-session".to_string(),
        project_id: None,
        turns: vec![],
        tags: vec![Tag::Text],
        total_tokens: 100,
        truncated: false,
        period: ArchivePeriod::Daily,
        access_count: 0,
        importance: 0,
        updates: vec![],
    };
    let memory_id = storage.write_memory(&file).await.unwrap();
    let read = storage.read_memory(&memory_id).await.unwrap();
    assert_eq!(read.id, file.id);
    assert_eq!(read.session_id, "test-session");
}

#[tokio::test]
async fn test_mock_storage_delete_memory() {
    let storage = mock_storage::MockStorage::new();
    let file = MemoryFile {
        id: Uuid::new_v4(),
        schema_version: 1,
        archived_at: Utc::now(),
        session_id: "test-session".to_string(),
        project_id: None,
        turns: vec![],
        tags: vec![Tag::Text],
        total_tokens: 100,
        truncated: false,
        period: ArchivePeriod::Daily,
        access_count: 0,
        importance: 0,
        updates: vec![],
    };
    let memory_id = storage.write_memory(&file).await.unwrap();
    storage.delete_memory(&memory_id).await.unwrap();
    let result = storage.read_memory(&memory_id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mock_storage_append_hook() {
    let storage = mock_storage::MockStorage::new();
    let hook = IndexHook {
        id: Uuid::new_v4(),
        memory_id: "mem-1".to_string(),
        summary: Summary { title: "测试".to_string(), abstract_text: None, key_facts: vec![], key_entities: vec![], clue_anchors: vec![] },
        tags: vec![Tag::Text],
        archived_at: Utc::now(),
        period: ArchivePeriod::Daily,
        token_count: 100,
        file_status: FileStatus::Normal,
        archive_reason: None,
        raw_context_path: None,
    };
    storage.append_hook("session-1", None, ArchivePeriod::Daily, hook.clone()).await.unwrap();
    let doc = storage.read_index("session-1", None, ArchivePeriod::Daily).await.unwrap();
    assert!(doc.is_some());
    assert_eq!(doc.unwrap().hooks.len(), 1);
}
