//! Error → JsValue 转换

use memory_center_core_logic::Error;
use wasm_bindgen::JsValue;

/// 将 core-logic Error 转换为 JS Error 对象
pub fn error_to_js(error: Error) -> JsValue {
    let (code, message) = match error {
        Error::Storage(msg) => ("STORAGE_ERROR", msg),
        Error::Serialize(msg) => ("SERIALIZE_ERROR", msg),
        Error::Index(msg) => ("INDEX_ERROR", msg),
        Error::Score(msg) => ("SCORE_ERROR", msg),
        Error::Migrate(msg) => ("MIGRATE_ERROR", msg),
    };
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &"code".into(), &code.into()).ok();
    js_sys::Reflect::set(&obj, &"message".into(), &message.into()).ok();
    JsValue::from(obj)
}

/// 从 JsValue 提取错误消息（JsStorage 回调用）
pub fn js_to_error_message(js: &JsValue) -> String {
    js.as_string().unwrap_or_else(|| {
        let msg = js_sys::Reflect::get(js, &"message".into()).ok();
        msg.and_then(|v| v.as_string()).unwrap_or_else(|| "未知 JS 错误".to_string())
    })
}
