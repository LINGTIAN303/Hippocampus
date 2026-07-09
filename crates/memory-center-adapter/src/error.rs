//! # 适配器错误类型（v2.46 新增）
//!
//! 统一不同 Agent adapter 的错误返回，让 sidecar 上层不依赖具体 Agent 的错误类型。
//!
//! ## 设计
//!
//! `AdapterError::Database(String)` 保存原始错误信息，
//! 具体 Agent 的内部错误类型通过 `From` trait 自动转换。
//! 例如 OpenCode 的 `DbError` 实现 `Into<AdapterError>`，
//! 在 `impl AgentAdapter for OpenCodeDb` 方法内 `.map_err()` 转换。

use thiserror::Error;

/// 适配器统一错误
#[derive(Debug, Error)]
pub enum AdapterError {
    /// 数据库错误（如 SQLite 查询失败、JSON 解析失败）
    ///
    /// 保存原始错误信息的字符串形式，不绑定具体 Agent 的错误类型。
    #[error("数据库错误: {0}")]
    Database(String),

    /// IO 错误（如文件不存在、权限不足）
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// 解析错误（如 JSON 反序列化失败、数据格式不符）
    #[error("解析错误: {0}")]
    Parse(String),

    /// 不支持的操作（如该 Agent 不支持某方法）
    #[error("不支持的操作: {0}")]
    Unsupported(String),
}
