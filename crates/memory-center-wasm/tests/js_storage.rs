//! JsStorage 回调机制测试
//!
//! 验证 JsStorage 的构造校验 + 回调往返 + 错误传播。
//! 通过 js_sys::eval 注入 mock 回调对象（基于 Map 的内存存储）。
//
//! 注意：JsStorage 仅在 wasm32 目标下编译（JsFuture 不 Send），
//! 本测试也只在 wasm32 下运行。

#![cfg(target_arch = "wasm32")]

use memory_center_core_logic::model::MemoryFile;
use memory_center_core_logic::storage::Storage;
use memory_center_wasm::JsStorage;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

// Node.js 是 wasm-bindgen-test 的默认执行环境，无需 wasm_bindgen_test_configure!

/// 构造 mock 回调对象（基于 Map 的内存存储）
fn create_mock_callbacks() -> JsValue {
    let js_code = r#"
    (function() {
        const memoryStore = new Map();
        const indexStore = new Map();
        const metaStore = new Map();
        const rawContextStore = new Map();

        return {
            writeMemory: async (file) => {
                const id = "memory-" + file.id;
                memoryStore.set(id, file);
                return id;
            },
            readMemory: async (id) => {
                return memoryStore.get(id) || null;
            },
            deleteMemory: async (id) => {
                if (!memoryStore.has(id)) throw new Error("not found: " + id);
                memoryStore.delete(id);
            },
            writeIndex: async (doc) => {
                const key = doc.session_id + ":" + (doc.project_id || "") + ":" + doc.period;
                indexStore.set(key, doc);
                return key;
            },
            readIndex: async ([sessionId, projectId, period]) => {
                const key = sessionId + ":" + (projectId || "") + ":" + period;
                return indexStore.get(key) || null;
            },
            appendHook: async ([sessionId, projectId, period, hook]) => {
                const key = sessionId + ":" + (projectId || "") + ":" + period;
                let doc = indexStore.get(key);
                if (!doc) {
                    doc = { session_id: sessionId, project_id: projectId, period: period, hooks: [] };
                    indexStore.set(key, doc);
                }
                doc.hooks.push(hook);
            },
            listMemories: async ([sessionId, projectId, period]) => {
                const result = [];
                for (const [id, file] of memoryStore.entries()) {
                    if (file.session_id === sessionId && file.period === period) {
                        result.push(id);
                    }
                }
                return result;
            },
            writeSessionMeta: async ([sessionId, meta]) => {
                metaStore.set(sessionId, meta);
            },
            readSessionMeta: async (sessionId) => {
                return metaStore.get(sessionId) || null;
            },
            writeRawContext: async ([sessionId, hookId, content]) => {
                const key = sessionId + ":" + hookId;
                rawContextStore.set(key, content);
                return "sessions/" + sessionId + "/raw_contexts/" + hookId + ".txt";
            },
            readRawContext: async ([sessionId, hookId]) => {
                const key = sessionId + ":" + hookId;
                if (!rawContextStore.has(key)) throw new Error("not found");
                return rawContextStore.get(key);
            },
            deleteRawContext: async ([sessionId, hookId]) => {
                const key = sessionId + ":" + hookId;
                rawContextStore.delete(key);
            },
        };
    })()
    "#;
    js_sys::eval(js_code).expect("eval mock callbacks failed")
}

/// 构造测试用 MemoryFile（含 1 个轮次）
fn make_test_memory_file() -> MemoryFile {
    use chrono::Utc;
    use memory_center_core_logic::model::*;
    use uuid::Uuid;

    let turn = MessageTurn {
        id: Uuid::new_v4(),
        user_message: MessageContent {
            text: Some("测试用户消息".to_string()),
            attachments: vec![],
            tool_calls: vec![],
            thinking: None,
        },
        llm_message: MessageContent {
            text: Some("测试 LLM 回复".to_string()),
            attachments: vec![],
            tool_calls: vec![],
            thinking: None,
        },
        tags: vec![Tag::Text],
        timestamp: Utc::now(),
        token_count: 10,
    };
    MemoryFile::new(
        "test-session",
        Some("test-project".to_string()),
        vec![turn],
        ArchivePeriod::Daily,
    )
}

#[wasm_bindgen_test]
async fn test_js_storage_new_valid_callbacks() {
    let callbacks = create_mock_callbacks();
    let result = JsStorage::new(callbacks);
    assert!(result.is_ok(), "有效的回调对象应该构造成功");
}

#[wasm_bindgen_test]
async fn test_js_storage_new_missing_callback() {
    // 空对象，缺少所有回调
    let bad_callbacks = js_sys::Object::new();
    let result = JsStorage::new(bad_callbacks.into());
    assert!(result.is_err(), "缺少回调应该返回错误");
}

#[wasm_bindgen_test]
async fn test_js_storage_write_read_memory() {
    let callbacks = create_mock_callbacks();
    let storage = JsStorage::new(callbacks).expect("构造 JsStorage 失败");

    let file = make_test_memory_file();
    let memory_id = storage.write_memory(&file).await.expect("write_memory 失败");
    assert!(
        memory_id.starts_with("memory-"),
        "memory_id 应以 'memory-' 开头，实际: {}",
        memory_id
    );

    let read = storage.read_memory(&memory_id).await.expect("read_memory 失败");
    assert_eq!(read.id, file.id, "read.id 应等于 file.id");
    assert_eq!(read.session_id, file.session_id);
    assert_eq!(read.project_id, file.project_id);
}

#[wasm_bindgen_test]
async fn test_js_storage_callback_error() {
    // 使用一个会 reject 的回调对象测试错误传播
    let js_code = r#"
    (function() {
        const badFn = async () => { throw new Error("mock error"); };
        return {
            writeMemory: badFn,
            readMemory: badFn,
            deleteMemory: badFn,
            writeIndex: badFn,
            readIndex: badFn,
            appendHook: badFn,
            listMemories: badFn,
            writeSessionMeta: badFn,
            readSessionMeta: badFn,
            writeRawContext: badFn,
            readRawContext: badFn,
            deleteRawContext: badFn,
        };
    })()
    "#;
    let callbacks = js_sys::eval(js_code).expect("eval bad callbacks failed");
    let storage = JsStorage::new(callbacks).expect("构造 JsStorage 失败");

    let file = make_test_memory_file();
    let result = storage.write_memory(&file).await;
    assert!(result.is_err(), "回调 reject 应该返回错误");
}
