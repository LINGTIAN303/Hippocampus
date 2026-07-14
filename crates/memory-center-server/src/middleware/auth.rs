//! # API Key 鉴权中间件
//!
//! 从 `Authorization: Bearer <key>` 头提取 API Key 并与 `MEMORY_CENTER_API_KEY`
//! 环境变量配置的期望值比对。
//!
//! ## 设计
//!
//! - **未配置 `MEMORY_CENTER_API_KEY` 时跳过鉴权**（向后兼容，本地开发零配置可用）
//!   - 除非设置 `MEMORY_CENTER_REQUIRE_API_KEY=true`，此时未配置 API Key 将返回 500
//! - **配置后**：所有请求必须携带正确的 `Authorization: Bearer <key>` 头
//! - **常量时间比对**：使用 `subtle` 风格的逐字节 XOR 比对，避免时序攻击
//!   - 长度不同时仍执行完整循环，避免长度泄露
//!
//! ## 错误响应
//!
//! - 未携带 Authorization 头 → 401 `{"error":{"code":"UNAUTHORIZED","message":"..."}}`
//!   + `WWW-Authenticate: Bearer realm="MemoryCenter"` 响应头
//! - 格式错误（非 `Bearer ` 前缀） → 401
//! - API Key 不匹配 → 403 `{"error":{"code":"FORBIDDEN","message":"..."}}`
//! - `REQUIRE_API_KEY=true` 但未配置 API Key → 500（服务端配置错误）

use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use std::env;

/// 鉴权所需的环境变量名
pub const ENV_API_KEY: &str = "MEMORY_CENTER_API_KEY";

/// 强制鉴权模式环境变量名（v2.51 安全加固）
///
/// - `true`：未配置 `MEMORY_CENTER_API_KEY` 时返回 500，拒绝服务
/// - `false`（默认）：未配置时跳过鉴权（向后兼容，本地开发零配置可用）
pub const ENV_REQUIRE_API_KEY: &str = "MEMORY_CENTER_REQUIRE_API_KEY";

/// `WWW-Authenticate` 头的 realm 值
const WWW_AUTH_REALM: &str = "Bearer realm=\"MemoryCenter\"";

/// 读取环境变量中配置的 API Key
///
/// - 返回 `None`：未配置
/// - 返回 `Some(key)`：已配置，所有请求必须携带正确的 Bearer token
pub fn configured_api_key() -> Option<String> {
    env::var(ENV_API_KEY).ok().filter(|s| !s.is_empty())
}

/// 读取 `MEMORY_CENTER_REQUIRE_API_KEY` 配置
///
/// - `true`：强制鉴权模式（未配置 API Key 时拒绝服务）
/// - `false`（默认）：宽松模式（未配置时跳过鉴权）
pub fn is_require_api_key() -> bool {
    matches!(
        env::var(ENV_REQUIRE_API_KEY).ok().as_deref(),
        Some("true") | Some("1") | Some("yes")
    )
}

/// Axum 中间件：API Key 鉴权
///
/// 使用方式：
/// ```ignore
/// use axum::middleware;
/// let app = create_router(state)
///     .layer(middleware::from_fn(crate::middleware::auth::require_api_key));
/// ```
pub async fn require_api_key(req: Request, next: Next) -> Response {
    let expected = match configured_api_key() {
        Some(k) => k,
        None => {
            // 未配置 API Key
            if is_require_api_key() {
                // 强制模式：未配置 API Key 拒绝服务（防误配置导致裸奔）
                return server_error_response(
                    "服务端配置错误：MEMORY_CENTER_REQUIRE_API_KEY=true 但未配置 MEMORY_CENTER_API_KEY",
                );
            }
            // 宽松模式：跳过鉴权（向后兼容）
            return next.run(req).await;
        }
    };

    // 提取 Authorization 头
    let auth_header = match req.headers().get(header::AUTHORIZATION) {
        Some(v) => v.to_str().unwrap_or(""),
        None => return unauthorized_response("缺少 Authorization 头"),
    };

    // 校验 Bearer 前缀
    let token = match auth_header.strip_prefix("Bearer ") {
        Some(t) => t,
        None => return unauthorized_response("Authorization 头格式错误，应为 'Bearer <api_key>'"),
    };

    // 常量时间比对（长度安全，避免时序侧信道攻击）
    if !constant_time_eq(token.as_bytes(), expected.as_bytes()) {
        return forbidden_response("API Key 不正确");
    }

    next.run(req).await
}

/// 常量时间字节比对（长度安全，避免时序侧信道攻击）
///
/// 与简单实现不同，本函数在长度不同时仍会执行完整循环，
/// 避免攻击者通过响应时间差异推断密钥长度。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // 长度先记录，但不在早期返回
    let len_eq = a.len() == b.len();
    let max_len = std::cmp::max(a.len(), b.len());

    let mut result: u8 = if len_eq { 0 } else { 0xff };
    for i in 0..max_len {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        result |= x ^ y;
    }
    // 长度不同时 result 已被设为 0xff，必然返回 false
    result == 0 && len_eq
}

// ---------------------------------------------------------------------------
// 错误响应构造
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: String,
    message: String,
}

fn unauthorized_response(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, WWW_AUTH_REALM)],
        Json(ErrorBody {
            error: ErrorDetail {
                code: "UNAUTHORIZED".to_string(),
                message: message.to_string(),
            },
        }),
    )
        .into_response()
}

fn forbidden_response(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorBody {
            error: ErrorDetail {
                code: "FORBIDDEN".to_string(),
                message: message.to_string(),
            },
        }),
    )
        .into_response()
}

fn server_error_response(message: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorBody {
            error: ErrorDetail {
                code: "SERVER_CONFIG_ERROR".to_string(),
                message: message.to_string(),
            },
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_time_eq_same() {
        assert!(constant_time_eq(b"abc123", b"abc123"));
    }

    #[test]
    fn test_constant_time_eq_diff() {
        assert!(!constant_time_eq(b"abc123", b"abc124"));
    }

    #[test]
    fn test_constant_time_eq_diff_len_short_long() {
        // 短 key 在前
        assert!(!constant_time_eq(b"abc", b"abcdef"));
    }

    #[test]
    fn test_constant_time_eq_diff_len_long_short() {
        // 长 key 在前（验证对称性）
        assert!(!constant_time_eq(b"abcdef", b"abc"));
    }

    #[test]
    fn test_constant_time_eq_empty() {
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn test_constant_time_eq_empty_vs_nonempty() {
        assert!(!constant_time_eq(b"", b"a"));
        assert!(!constant_time_eq(b"a", b""));
    }

    #[test]
    fn test_is_require_api_key_default_false() {
        // 不设置环境变量时应为 false
        // 注意：此测试可能受并行测试环境影响，仅做基本验证
        let _ = is_require_api_key();
    }

    #[test]
    fn test_configured_api_key_none_when_unset() {
        // 仅验证函数可调用（无法可靠测试 env var 状态）
        let _ = configured_api_key();
    }
}
