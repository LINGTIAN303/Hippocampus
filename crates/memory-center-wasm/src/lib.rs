//! # MemoryCenter WASM
//!
//! WASM 绑定层：将 MemoryCenter-core-logic 编译为 WASM，提供 JS 调用 API。

#![forbid(unsafe_code)]

pub mod error;
pub mod memory_storage;
#[cfg(target_arch = "wasm32")]
pub mod js_storage;
pub mod bindings;

// Task 8-10 启用
pub use memory_storage::MemoryStorage;
#[cfg(target_arch = "wasm32")]
pub use js_storage::JsStorage;
pub use bindings::MemoryCenterCore;
