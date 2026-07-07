// crates/hippocampus-core-logic/src/lib.rs
//! # Hippocampus Core Logic
//!
//! 核心逻辑 + Storage trait 定义，无原生 IO 依赖，可编译为 WASM。

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod archive;
// BM25 模块：native 模式用 jieba 中文分词，WASM 模式用简易字符分词
// 两者公共接口完全一致（Bm25Searcher + KeywordSearcher trait impl）
#[cfg(feature = "native")]
pub mod bm25;
#[cfg(not(feature = "native"))]
#[path = "bm25_wasm.rs"]
pub mod bm25;
pub mod compact;
pub mod conflict;
pub mod context_parser;
pub mod generate;
pub mod heuristic;
pub mod hybrid;
pub mod migrator;
pub mod model;
pub mod retrieve;
pub mod score;
pub mod semantic;
pub mod serialization;
pub mod storage;
pub mod vector;

#[cfg(test)]
pub mod test_support;

/// Crate 级错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("存储错误: {0}")]
    Storage(String),
    #[error("序列化错误: {0}")]
    Serialize(String),
    #[error("索引错误: {0}")]
    Index(String),
    #[error("评分错误: {0}")]
    Score(String),
    #[error("迁移错误: {0}")]
    Migrate(String),
}

pub type Result<T> = std::result::Result<T, Error>;
