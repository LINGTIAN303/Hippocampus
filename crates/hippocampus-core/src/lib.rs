//! # Hippocampus Core
//!
//! Agent 记忆库的核心逻辑库。
//!
//! ## 模块组织（v2.35 WASM 拆分后）
//!
//! - **纯逻辑层**（来自 `hippocampus-core-logic` crate，WASM 兼容）：
//!   - [`model`]：核心数据模型（记忆文件、索引钩子、标签等）
//!   - [`score`]：评分 trait + 默认启发式实现
//!   - [`storage`] trait：存储后端接口（trait 部分）
//!   - [`serialization`]：序列化格式（JSON / MessagePack 双格式支持）
//!   - [`migrator`]：Schema 版本迁移
//!   - [`bm25`] / [`conflict`] / [`heuristic`] / [`semantic`] / [`vector`] / [`generate`] / [`context_parser`]
//! - **IO 实现层**（本 crate 独有，依赖原生 fs/SQLite）：
//!   - [`archive`]：归档/冻结逻辑（达到阈值时将上下文冻结为记忆文件）
//!   - [`retrieve`]：检索机制（摘要钩子注入 + tool 主动检索）
//!   - [`compact`]：周期任务（周去重合并 / 月评分淘汰）
//!   - [`storage`] LocalStorage：本地文件树实现
//!   - [`sqlite`]：SQLite 存储后端（rusqlite + r2d2 连接池 + WAL 模式）
//!   - [`cache`]：缓存装饰器（CachedStorage<T>，moka LRU + TTL）
//!   - [`hybrid`]：混合检索器（HybridRetriever + RRF 融合 + 降级策略）
//!
//! ## 索引管理职责分配
//!
//! 「索引文档」与「索引钩子」的职责由多个模块共同承担，不设独立的 IndexManager：
//! - **数据模型**：[`model::IndexDocument`] / [`model::IndexHook`]
//! - **持久化**：[`storage::Storage`] trait 的 `append_hook` / `read_index` / `write_index`
//! - **摘要渲染**：[`retrieve::Retriever`] 的 `render_to_system_prompt`
//! - **钩子检索**：[`retrieve::Retriever`] 的 `retrieve_memory`
//! - **周期合并**：[`compact::Compactor`] 的 `weekly_merge` / `monthly_evict`（钩子迁移）
//!
//! ## 核心概念
//!
//! - **归档（freeze）**：达到 token 阈值时，将完整上下文（用户消息+LLM消息）保存为记忆文件，非摘要
//! - **索引钩子（hook）**：指向记忆库中记忆文件的指针，带 17 类细粒度标签
//! - **三级周期**：天级归档 / 周级无损去重合并 / 月级评分淘汰

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

// ============================================================================
// IO 实现层模块（hippocampus-core 独有，依赖原生 fs/SQLite）
// ============================================================================
/// 缓存装饰器（CachedStorage<T>，moka LRU + TTL）
pub mod cache;
/// SQLite 存储后端（rusqlite + r2d2 连接池 + WAL 模式）
pub mod sqlite;
/// SQLite 向量索引（BLOB 持久化 + InMemoryVectorIndex 缓存）
pub mod sqlite_vector;
/// 本地文件树存储后端实现（Storage trait 来自 core-logic）
pub mod storage;

// ============================================================================
// 从 hippocampus-core-logic 重导出纯逻辑模块（v2.35 WASM 拆分）
// ============================================================================
//
// 所有纯逻辑模块的源文件已迁移到 hippocampus-core-logic/src/ 中，
// hippocampus-core/src/ 中已删除这些文件，lib.rs 通过 `pub use` 重导出。
// 这样 `crate::model::*` / `crate::bm25::*` / `crate::archive::*` 等在
// hippocampus-core 中指向 core-logic 的版本，与 Storage trait 中的
// `crate::model::*` 类型一致。

/// 归档/冻结逻辑（达到阈值时将上下文冻结为记忆文件）
pub use hippocampus_core_logic::archive;
/// BM25 关键词检索（jieba-rs 中文分词 + 倒排索引）
pub use hippocampus_core_logic::bm25;
/// 周期任务（周去重合并 / 月评分淘汰）
pub use hippocampus_core_logic::compact;
/// 记忆冲突检测（ConflictDetector trait + NoopDetector）
pub use hippocampus_core_logic::conflict;
/// 上下文字符串解析器（v2.34，pre_compress_hook 用）
pub use hippocampus_core_logic::context_parser;
/// LLM 生成器 trait（AnchorGenerator / SummaryGenerator，v2.16 IMP-05/10）
pub use hippocampus_core_logic::generate;
/// 启发式冲突检测器（HeuristicDetector，反义词词典 + 三维度检测）
pub use hippocampus_core_logic::heuristic;
/// 混合检索器（HybridRetriever + RRF 融合 + 降级策略）
pub use hippocampus_core_logic::hybrid;
pub use hippocampus_core_logic::migrator;
pub use hippocampus_core_logic::model;
/// 检索机制（摘要钩子注入 + tool 主动检索）
pub use hippocampus_core_logic::retrieve;
pub use hippocampus_core_logic::score;
/// 语义检索（Embedder / KeywordSearcher / VectorIndex / SemanticRetriever trait + RRF 融合）
pub use hippocampus_core_logic::semantic;
/// 序列化格式（JSON / MessagePack 双格式支持）
pub use hippocampus_core_logic::serialization;
/// 向量索引（InMemoryVectorIndex + cosine_similarity）
pub use hippocampus_core_logic::vector;

// ============================================================================
// 从 core-logic 重导出 Error / Result（保持 `crate::Error` / `crate::Result` 可用）
// ============================================================================
//
// hippocampus-core 不再自定义 Error / Result，而是从 core-logic 重导出。
// 这样 Storage trait 中的 `crate::Result` / `crate::Error`（定义在 core-logic）
// 与 hippocampus-core 中 LocalStorage impl 的 `crate::Result` / `crate::Error`
// 指向同一类型，避免类型不匹配。

/// Crate 级错误类型（从 core-logic 重导出）
pub use hippocampus_core_logic::Error;

/// Crate 级结果别名（从 core-logic 重导出）
pub use hippocampus_core_logic::Result;

// ============================================================================
// 显式重导出 Storage trait 和 SessionMeta
// ============================================================================
//
// 保持 `use hippocampus_core::storage::Storage` / `use hippocampus_core::Storage` 可用。
// hippocampus-core/src/storage.rs 中只有 LocalStorage 实现，trait 来自 core-logic。

/// Storage trait（存储后端接口，来自 core-logic）
pub use hippocampus_core_logic::storage::Storage;

/// SessionMeta（session 元数据，来自 core-logic）
pub use hippocampus_core_logic::storage::SessionMeta;
