//! JsStorage - 注入式 Storage 实现
//!
//! JS 调用方实现 Storage trait 的所有方法，通过回调注入。
//! 适用于 IndexedDB / Workers KV / fetch 到远程服务等场景。
//!
//! ## 回调约定
//!
//! JS 端传入一个对象，包含以下方法（均返回 Promise）：
//! - `writeMemory(file)` → Promise<string>
//! - `readMemory(id)` → Promise<MemoryFile | null>
//! - `deleteMemory(id)` → Promise<void>
//! - `writeIndex(doc)` → Promise<string>
//! - `readIndex([sessionId, projectId, period])` → Promise<IndexDocument | null>
//! - `appendHook([sessionId, projectId, period, hook])` → Promise<void>
//! - `listMemories([sessionId, projectId, period])` → Promise<string[]>
//! - `writeSessionMeta([sessionId, meta])` → Promise<void>
//! - `readSessionMeta(sessionId)` → Promise<SessionMeta | null>
//! - `writeRawContext([sessionId, hookId, content])` → Promise<string>
//! - `readRawContext([sessionId, hookId])` → Promise<string>
//! - `deleteRawContext([sessionId, hookId])` → Promise<void>
//!
//! 所有 Promise reject 会被转为 [`Error::Storage`]。

use memory_center_core_logic::model::{ArchivePeriod, IndexDocument, IndexHook, MemoryFile};
use memory_center_core_logic::storage::{SessionMeta, Storage};
use memory_center_core_logic::{Error, Result as CoreResult};
use js_sys::Function;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// JS 回调注入式 Storage 实现
///
/// 通过 `JsStorage::new(callbacks)` 构造，`callbacks` 为包含 12 个回调方法的 JS 对象。
/// 适用于将存储后端逻辑下放到 JS 侧（如 IndexedDB / Workers KV / 远程服务）。
#[wasm_bindgen]
pub struct JsStorage {
    write_memory_fn: Function,
    read_memory_fn: Function,
    delete_memory_fn: Function,
    write_index_fn: Function,
    read_index_fn: Function,
    append_hook_fn: Function,
    list_memories_fn: Function,
    write_session_meta_fn: Function,
    read_session_meta_fn: Function,
    write_raw_context_fn: Function,
    read_raw_context_fn: Function,
    delete_raw_context_fn: Function,
}

#[wasm_bindgen]
impl JsStorage {
    /// 创建 JsStorage
    ///
    /// `callbacks` 为 JS 对象，需包含全部 12 个回调方法（见模块文档）。
    /// 缺少任一方法或字段类型不是 function 时返回 JsValue 错误。
    ///
    /// 注意：返回类型是 `std::result::Result<JsStorage, JsValue>`（非 core_logic::Result），
    /// 因为 wasm_bindgen 要求构造函数返回 `Result<T, JsValue>`。
    #[wasm_bindgen(constructor)]
    pub fn new(callbacks: JsValue) -> std::result::Result<JsStorage, JsValue> {
        let obj = callbacks
            .dyn_into::<js_sys::Object>()
            .map_err(|_| JsValue::from("callbacks 必须是对象"))?;
        let get_fn = |key: &str| -> std::result::Result<Function, JsValue> {
            let key_val = JsValue::from(key);
            let val = js_sys::Reflect::get(&obj, &key_val)?;
            val.dyn_into::<Function>()
                .map_err(|_| JsValue::from(format!("回调 {} 不是函数", key)))
        };
        Ok(JsStorage {
            write_memory_fn: get_fn("writeMemory")?,
            read_memory_fn: get_fn("readMemory")?,
            delete_memory_fn: get_fn("deleteMemory")?,
            write_index_fn: get_fn("writeIndex")?,
            read_index_fn: get_fn("readIndex")?,
            append_hook_fn: get_fn("appendHook")?,
            list_memories_fn: get_fn("listMemories")?,
            write_session_meta_fn: get_fn("writeSessionMeta")?,
            read_session_meta_fn: get_fn("readSessionMeta")?,
            write_raw_context_fn: get_fn("writeRawContext")?,
            read_raw_context_fn: get_fn("readRawContext")?,
            delete_raw_context_fn: get_fn("deleteRawContext")?,
        })
    }
}

/// 调用 JS 函数并 await Promise，reject 转为 `Error::Storage`
async fn call_js_fn(fn_ref: &Function, arg: &JsValue) -> CoreResult<JsValue> {
    let promise_val = fn_ref
        .call1(&JsValue::NULL, arg)
        .map_err(|e| Error::Storage(format!("JS 回调调用失败: {:?}", e)))?;
    let promise = js_sys::Promise::from(promise_val);
    JsFuture::from(promise)
        .await
        .map_err(|e| Error::Storage(format!("JS 回调返回错误: {:?}", e)))
}

/// 序列化为 JsValue，失败转为 `Error::Serialize`
fn to_js<T: serde::Serialize>(value: &T) -> CoreResult<JsValue> {
    serde_wasm_bindgen::to_value(value)
        .map_err(|e| Error::Serialize(format!("序列化失败: {:?}", e)))
}

/// 从 JsValue 反序列化，失败转为 `Error::Serialize`
fn from_js<T: serde::de::DeserializeOwned>(value: JsValue) -> CoreResult<T> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| Error::Serialize(format!("反序列化失败: {:?}", e)))
}

// JsStorage 在 WASM 下用 ?Send（JsFuture 不 Send），native 下用 Send 以匹配 Storage trait 定义
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl Storage for JsStorage {
    async fn write_memory(&self, file: &MemoryFile) -> CoreResult<String> {
        let js_obj = to_js(file)?;
        let result = call_js_fn(&self.write_memory_fn, &js_obj).await?;
        result
            .as_string()
            .ok_or_else(|| Error::Serialize("writeMemory 回调返回值不是 string".into()))
    }

    async fn read_memory(&self, memory_id: &str) -> CoreResult<MemoryFile> {
        let result = call_js_fn(&self.read_memory_fn, &JsValue::from(memory_id)).await?;
        from_js(result)
    }

    async fn delete_memory(&self, memory_id: &str) -> CoreResult<()> {
        call_js_fn(&self.delete_memory_fn, &JsValue::from(memory_id)).await?;
        Ok(())
    }

    async fn write_index(&self, doc: &IndexDocument) -> CoreResult<String> {
        let js_obj = to_js(doc)?;
        let result = call_js_fn(&self.write_index_fn, &js_obj).await?;
        result
            .as_string()
            .ok_or_else(|| Error::Serialize("writeIndex 回调返回值不是 string".into()))
    }

    async fn read_index(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
    ) -> CoreResult<Option<IndexDocument>> {
        let args = to_js(&(session_id, project_id, period))?;
        let result = call_js_fn(&self.read_index_fn, &args).await?;
        if result.is_null() || result.is_undefined() {
            Ok(None)
        } else {
            Ok(Some(from_js(result)?))
        }
    }

    async fn append_hook(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
        hook: IndexHook,
    ) -> CoreResult<()> {
        let args = to_js(&(session_id, project_id, period, &hook))?;
        call_js_fn(&self.append_hook_fn, &args).await?;
        Ok(())
    }

    async fn list_memories(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
    ) -> CoreResult<Vec<String>> {
        let args = to_js(&(session_id, project_id, period))?;
        let result = call_js_fn(&self.list_memories_fn, &args).await?;
        from_js(result)
    }

    async fn write_session_meta(&self, session_id: &str, meta: &SessionMeta) -> CoreResult<()> {
        let args = to_js(&(session_id, meta))?;
        call_js_fn(&self.write_session_meta_fn, &args).await?;
        Ok(())
    }

    async fn read_session_meta(&self, session_id: &str) -> CoreResult<Option<SessionMeta>> {
        let result = call_js_fn(&self.read_session_meta_fn, &JsValue::from(session_id)).await?;
        if result.is_null() || result.is_undefined() {
            Ok(None)
        } else {
            Ok(Some(from_js(result)?))
        }
    }

    async fn write_raw_context(
        &self,
        session_id: &str,
        hook_id: &str,
        content: &str,
    ) -> CoreResult<String> {
        let args = to_js(&(session_id, hook_id, content))?;
        let result = call_js_fn(&self.write_raw_context_fn, &args).await?;
        result
            .as_string()
            .ok_or_else(|| Error::Serialize("writeRawContext 回调返回值不是 string".into()))
    }

    async fn read_raw_context(&self, session_id: &str, hook_id: &str) -> CoreResult<String> {
        let args = to_js(&(session_id, hook_id))?;
        let result = call_js_fn(&self.read_raw_context_fn, &args).await?;
        result
            .as_string()
            .ok_or_else(|| Error::Serialize("readRawContext 回调返回值不是 string".into()))
    }

    async fn delete_raw_context(&self, session_id: &str, hook_id: &str) -> CoreResult<()> {
        let args = to_js(&(session_id, hook_id))?;
        call_js_fn(&self.delete_raw_context_fn, &args).await?;
        Ok(())
    }
}
