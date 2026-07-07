//! # MemoryCenter 搜索层
//!
//! v2.18 批次2：从 memory-center-server 下沉，隔离 axum 重依赖。
//!
//! 包含两个核心组件：
//!
//! - [`SearchIndexer`]：归档后自动索引（v2.5 批次 7，全局单例，已废弃保留兼容）
//! - [`SessionSearchRouter`]：Session 级索引隔离路由器（v2.8+，含 LRU/TTL 管理 + 懒重建）
//!
//! ## 设计目标
//!
//! - **不依赖 HTTP 框架**：纯搜索逻辑，供 server/mcp/未来 WASM 复用
//! - **Session 级隔离**：每个 session 独立的 BM25 + 向量索引
//! - **懒重建**：首次访问 session 时从 storage 批量重建索引（v2.18 批次1）
//!
//! ## 架构
//!
//! ```text
//! archive handler (server/mcp)
//!   │
//!   └─→ SessionSearchRouter.index_hook(sid, hook)
//!         │
//!         ├─→ keyword.index(hook_id, text)   ← BM25
//!         └─→ embedder.embed → vector.add   ← 向量
//!
//! search handler (server/mcp)
//!   │
//!   └─→ SessionSearchRouter.search_with_rebuild(sid, query, top_k)
//!         │
//!         ├─→ 首次访问：从 storage 批量 read_hook → embed_batch → 装入索引
//!         └─→ retriever.search(query, top_k) ← HybridRetriever / KeywordOnlyRetriever
//! ```

/// v2.5 批次 7: 搜索索引器（归档后自动索引到 BM25 + 向量索引）
pub mod search;
/// v2.8: Session 级索引隔离路由器
pub mod session_search;

pub use search::SearchIndexer;
pub use session_search::{SessionSearchRouter, SessionSearchRouterConfig};
