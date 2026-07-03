//! # 存储后端对比基准
//!
//! 对比 LocalStorage vs SqliteStorage 在相同操作下的性能：
//! - archive（归档）
//! - retrieve（检索）
//! - get_summaries（摘要列表）
//!
//! 运行方式：`cargo bench -p hippocampus-bench --bench backend_compare`

use criterion::{criterion_group, criterion_main, Criterion};
use hippocampus_core::{
    archive::Archiver,
    model::{ArchiveConfig, MessageContent, MessageTurn, Tag},
    retrieve::Retriever,
    sqlite::SqliteStorage,
    storage::{LocalStorage, Storage},
};
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;

/// 构造测试用 MessageTurn
fn make_turn(text: &str, token_count: usize) -> MessageTurn {
    MessageTurn {
        id: Uuid::new_v4(),
        user_message: MessageContent {
            text: Some(text.into()),
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            thinking: None,
        },
        llm_message: MessageContent {
            text: Some("LLM 回复".into()),
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            thinking: None,
        },
        tags: vec![Tag::Text],
        timestamp: chrono::Utc::now(),
        token_count,
    }
}

/// 归档基准对比：Local vs SQLite
fn bench_archive_compare(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let turns: Vec<MessageTurn> = (0..50)
        .map(|i| make_turn(&format!("消息 #{}", i), 100 + i))
        .collect();

    let mut group = c.benchmark_group("archive_compare");

    group.bench_function("local_json", |b| {
        b.iter(|| {
            rt.block_on(async {
                let tmp = TempDir::new().unwrap();
                let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));
                let config = ArchiveConfig::default();
                let mut archiver = Archiver::new(config, storage, "bench-local", None);
                for turn in turns.clone() {
                    archiver.push_turn(turn);
                }
                archiver.archive().await.unwrap();
            });
        });
    });

    group.bench_function("sqlite", |b| {
        b.iter(|| {
            rt.block_on(async {
                let tmp = TempDir::new().unwrap();
                let storage: Arc<dyn Storage> = Arc::new(
                    SqliteStorage::new(tmp.path(), None).unwrap(),
                );
                let config = ArchiveConfig::default();
                let mut archiver = Archiver::new(config, storage, "bench-sqlite", None);
                for turn in turns.clone() {
                    archiver.push_turn(turn);
                }
                archiver.archive().await.unwrap();
            });
        });
    });

    group.finish();
}

/// 检索基准对比：Local vs SQLite
fn bench_retrieve_compare(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Local 预置
    let local_tmp = TempDir::new().unwrap();
    let local_storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(local_tmp.path()));
    let local_hook = rt.block_on(async {
        let config = ArchiveConfig::default();
        let mut archiver = Archiver::new(config, local_storage.clone(), "bench-ret", None);
        for i in 0..50 {
            archiver.push_turn(make_turn(&format!("消息 #{}", i), 100 + i));
        }
        let (_, hook) = archiver.archive().await.unwrap();
        hook.id.to_string()
    });

    // SQLite 预置
    let sqlite_tmp = TempDir::new().unwrap();
    let sqlite_storage: Arc<dyn Storage> = Arc::new(
        SqliteStorage::new(sqlite_tmp.path(), None).unwrap(),
    );
    let sqlite_hook = rt.block_on(async {
        let config = ArchiveConfig::default();
        let mut archiver = Archiver::new(config, sqlite_storage.clone(), "bench-ret", None);
        for i in 0..50 {
            archiver.push_turn(make_turn(&format!("消息 #{}", i), 100 + i));
        }
        let (_, hook) = archiver.archive().await.unwrap();
        hook.id.to_string()
    });

    let mut group = c.benchmark_group("retrieve_compare");

    group.bench_function("local_json", |b| {
        b.iter(|| {
            rt.block_on(async {
                let retriever = Retriever::new(local_storage.clone(), "bench-ret", None);
                retriever.retrieve_memory(&local_hook).await.unwrap();
            });
        });
    });

    group.bench_function("sqlite", |b| {
        b.iter(|| {
            rt.block_on(async {
                let retriever = Retriever::new(sqlite_storage.clone(), "bench-ret", None);
                retriever.retrieve_memory(&sqlite_hook).await.unwrap();
            });
        });
    });

    group.finish();
}

/// 摘要列表基准对比：Local vs SQLite
fn bench_summaries_compare(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Local 预置 50 个钩子
    let local_tmp = TempDir::new().unwrap();
    let local_storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(local_tmp.path()));
    rt.block_on(async {
        for _ in 0..50 {
            let config = ArchiveConfig::default();
            let mut archiver = Archiver::new(config, local_storage.clone(), "bench-sum", None);
            for i in 0..2 {
                archiver.push_turn(make_turn(&format!("消息 #{}", i), 100 + i));
            }
            archiver.archive().await.unwrap();
        }
    });

    // SQLite 预置 50 个钩子
    let sqlite_tmp = TempDir::new().unwrap();
    let sqlite_storage: Arc<dyn Storage> = Arc::new(
        SqliteStorage::new(sqlite_tmp.path(), None).unwrap(),
    );
    rt.block_on(async {
        for _ in 0..50 {
            let config = ArchiveConfig::default();
            let mut archiver = Archiver::new(config, sqlite_storage.clone(), "bench-sum", None);
            for i in 0..2 {
                archiver.push_turn(make_turn(&format!("消息 #{}", i), 100 + i));
            }
            archiver.archive().await.unwrap();
        }
    });

    let mut group = c.benchmark_group("summaries_compare");

    group.bench_function("local_json_50_hooks", |b| {
        b.iter(|| {
            rt.block_on(async {
                let retriever = Retriever::new(local_storage.clone(), "bench-sum", None);
                retriever.get_summaries().await.unwrap();
            });
        });
    });

    group.bench_function("sqlite_50_hooks", |b| {
        b.iter(|| {
            rt.block_on(async {
                let retriever = Retriever::new(sqlite_storage.clone(), "bench-sum", None);
                retriever.get_summaries().await.unwrap();
            });
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_archive_compare,
    bench_retrieve_compare,
    bench_summaries_compare,
);
criterion_main!(benches);
