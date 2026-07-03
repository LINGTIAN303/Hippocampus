//! # Hippocampus 基准测试与并发测试
//!
//! 独立的 bench crate，包含：
//!
//! - **基准测试**（`benches/` 目录）：用 criterion 测量核心操作性能
//!   - `core_operations`：归档/检索/更新/prompt 渲染基准
//!   - `backend_compare`：LocalStorage vs SqliteStorage 对比
//!   - `format_compare`：JSON vs MessagePack 序列化对比
//!
//! - **并发正确性测试**（`tests/` 目录）：验证并发场景下的正确性
//!   - `concurrent_archive`：多会话并发归档
//!   - `concurrent_read_write`：同会话读写并发
//!   - `concurrent_update`：同记忆并发更新
//!   - `sqlite_pool_stress`：SQLite 连接池压力测试
//!
//! 运行方式：
//! - 基准测试：`cargo bench -p hippocampus-bench`
//! - 并发测试：`cargo test -p hippocampus-bench --test concurrent_archive`
