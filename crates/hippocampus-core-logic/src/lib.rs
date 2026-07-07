// crates/hippocampus-core-logic/src/lib.rs
//! # Hippocampus Core Logic
//!
//! 核心逻辑 + Storage trait 定义，无原生 IO 依赖，可编译为 WASM。

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

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
