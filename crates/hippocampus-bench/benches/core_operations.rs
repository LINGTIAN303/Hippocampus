//! # 核心操作基准测试
//!
//! 测量 Hippocampus 核心操作的性能基线：
//! - archive（归档）：小规模（10 turns）+ 大规模（1000 turns）
//! - retrieve（检索）：单个记忆文件
//! - get_summaries（摘要列表）：100 个钩子
//! - render_prompt（渲染 system prompt）
//! - update_memory（PATCH 更新）
//!
//! 运行方式：`cargo bench -p hippocampus-bench --bench core_operations`

use criterion::{criterion_group, criterion_main, Criterion};
use hippocampus_core::{
    archive::Archiver,
    model::{ArchiveConfig, MessageContent, MessageTurn, MemoryUpdate, Tag},
    retrieve::Retriever,
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
            text: Some("LLM 回复内容".into()),
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            thinking: None,
        },
        tags: vec![Tag::Text, Tag::CodeBlock],
        timestamp: chrono::Utc::now(),
        token_count,
    }
}

/// 构造一批 turns
fn make_turns(n: usize, base_tokens: usize) -> Vec<MessageTurn> {
    (0..n)
        .map(|i| make_turn(&format!("用户消息 #{}", i), base_tokens + i))
        .collect()
}

/// 归档基准：小规模（10 turns）
fn bench_archive_small(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("archive_small_10_turns", |b| {
        b.iter(|| {
            rt.block_on(async {
                let tmp = TempDir::new().unwrap();
                let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));
                let config = ArchiveConfig {
                    token_threshold: 100_000,
                    force_truncate_limit: 150_000,
                    wait_for_turn_completion: true,
                };
                let mut archiver = Archiver::new(config, storage, "bench-sess", None);
                for turn in make_turns(10, 100) {
                    archiver.push_turn(turn);
                }
                archiver.archive().await.unwrap();
            });
        });
    });
}

/// 归档基准：大规模（1000 turns）
fn bench_archive_large(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("archive_large_1000_turns", |b| {
        b.iter(|| {
            rt.block_on(async {
                let tmp = TempDir::new().unwrap();
                let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));
                let config = ArchiveConfig {
                    token_threshold: 1_000_000,
                    force_truncate_limit: 1_500_000,
                    wait_for_turn_completion: true,
                };
                let mut archiver = Archiver::new(config, storage, "bench-sess", None);
                for turn in make_turns(1000, 100) {
                    archiver.push_turn(turn);
                }
                archiver.archive().await.unwrap();
            });
        });
    });
}

/// 检索基准：单个记忆文件
fn bench_retrieve(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));

    // 预先归档
    let hook_id = rt.block_on(async {
        let config = ArchiveConfig::default();
        let mut archiver = Archiver::new(config, storage.clone(), "bench-ret", None);
        for turn in make_turns(50, 100) {
            archiver.push_turn(turn);
        }
        let (_, hook) = archiver.archive().await.unwrap();
        hook.id.to_string()
    });

    c.bench_function("retrieve_memory", |b| {
        b.iter(|| {
            rt.block_on(async {
                let retriever = Retriever::new(storage.clone(), "bench-ret", None);
                retriever.retrieve_memory(&hook_id).await.unwrap();
            });
        });
    });
}

/// 摘要列表基准：100 个钩子
fn bench_get_summaries(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));

    // 预先归档 100 次
    rt.block_on(async {
        for _ in 0..100 {
            let config = ArchiveConfig::default();
            let mut archiver = Archiver::new(config, storage.clone(), "bench-sum", None);
            for turn in make_turns(2, 100) {
                archiver.push_turn(turn);
            }
            archiver.archive().await.unwrap();
        }
    });

    c.bench_function("get_summaries_100_hooks", |b| {
        b.iter(|| {
            rt.block_on(async {
                let retriever = Retriever::new(storage.clone(), "bench-sum", None);
                retriever.get_summaries().await.unwrap();
            });
        });
    });
}

/// Prompt 渲染基准
fn bench_render_prompt(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));

    // 预先归档 50 次
    rt.block_on(async {
        for _ in 0..50 {
            let config = ArchiveConfig::default();
            let mut archiver = Archiver::new(config, storage.clone(), "bench-prompt", None);
            for turn in make_turns(2, 100) {
                archiver.push_turn(turn);
            }
            archiver.archive().await.unwrap();
        }
    });

    c.bench_function("render_prompt_50_hooks", |b| {
        b.iter(|| {
            rt.block_on(async {
                let retriever = Retriever::new(storage.clone(), "bench-prompt", None);
                retriever.render_to_system_prompt().await.unwrap();
            });
        });
    });
}

/// 更新基准：PATCH 单个记忆
fn bench_update_memory(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));

    // 预先归档
    let memory_id = rt.block_on(async {
        let config = ArchiveConfig::default();
        let mut archiver = Archiver::new(config, storage.clone(), "bench-upd", None);
        for turn in make_turns(10, 100) {
            archiver.push_turn(turn);
        }
        let (_, hook) = archiver.archive().await.unwrap();
        hook.memory_id
    });

    c.bench_function("update_memory", |b| {
        b.iter(|| {
            rt.block_on(async {
                let updates = MemoryUpdate::new()
                    .add_fact("基准测试新增事实")
                    .revise_fact("基准测试修正事实");
                storage.update_memory(&memory_id, updates).await.unwrap();
            });
        });
    });
}

criterion_group!(
    benches,
    bench_archive_small,
    bench_archive_large,
    bench_retrieve,
    bench_get_summaries,
    bench_render_prompt,
    bench_update_memory,
);
criterion_main!(benches);
