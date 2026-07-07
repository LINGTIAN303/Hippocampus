# Crate Guide

MemoryCenter 采用 Cargo workspace 多 crate 架构，将核心逻辑、接口适配、语言绑定与辅助工具分离，让用户按需引入，避免无用依赖。本页帮助你快速选出合适的 crate。

## 1. 概览

整个仓库分为 4 类、共 19+ 个 crate（含 2 个计划中的语言绑定）：

| 类别 | 数量 | 职责 |
|------|------|------|
| 核心层 | 2 | 纯逻辑与原生 IO 整合 |
| 接口层 | 4 | 暴露给外部应用的接入点（FFI / HTTP / MCP / Python） |
| 绑定层 | 3 已实现 + 2 计划 | 面向特定语言的薄包装 |
| 工具层 | 9 | 预设 / 模型 / LLM / 搜索 / 技能 / 窗口 / 基准 |

设计原则：

- **核心零 IO**：`memory-center-core-logic` 不依赖文件系统、网络、异步运行时，可编译为 WASM
- **Facade 整合原生实现**：`memory-center-core` 重导出 core-logic 并保留 SQLite / 文件树存储
- **接口层薄**：FFI / Server / MCP / Python 只做协议适配，不重复业务逻辑
- **工具层独立**：预设、模型、LLM 等可单独引用，也可被 server / mcp 组合使用

## 2. Crate 选择决策树

```
你的场景？
├─ 需要嵌入宿主进程（C/C++/Rust 应用）
│   └─ memory-center-ffi（C ABI 动态库）
│
├─ 需要远程访问 / 多语言客户端共用
│   └─ memory-center-server（Axum HTTP REST + MCP Streamable HTTP）
│
├─ 本地 IDE Agent 接入（Claude Code / Cursor / Trae / Codex CLI）
│   └─ memory-center-mcp（stdio 传输，零配置）
│
├─ Python 应用（脚本 / FastAPI / Jupyter）
│   └─ memory-center-python（PyO3 原生绑定）
│
├─ 浏览器 / Edge runtime / 浏览器扩展
│   └─ memory-center-wasm（wasm-bindgen）
│
├─ Node.js 应用
│   └─ memory-center-node（已实现 v2.14，napi-rs 异步 Promise API）
├─ Go / Java 应用
│   └─ memory-center-go | memory-center-java（计划中，复用 C ABI）
│
└─ 只需要纯逻辑（无 IO，自定义存储实现）
    └─ memory-center-core-logic（直接依赖）
```

简化判断：

| 你想做什么 | 选择 |
|------------|------|
| 给 IDE Agent 加记忆能力 | `memory-center-mcp` |
| 给 Web 服务加记忆能力 | `memory-center-server` |
| 给 C/C++/Rust 应用嵌入记忆 | `memory-center-ffi` |
| 给 Python 脚本加记忆能力 | `memory-center-python` |
| 给浏览器扩展加记忆能力 | `memory-center-wasm` |
| 自研存储后端 / 纯算法复用 | `memory-center-core-logic` |

## 3. 核心层（2 个）

| Crate | 用途 | 何时使用 |
|-------|------|----------|
| `memory-center-core-logic` | 纯逻辑核心（数据模型 / 归档 / 索引 / 检索 / 评分 / BM25 / 语义检索），无 IO 依赖，可编译为 WASM | 自研存储后端、需要复用算法、目标平台为 WASM |
| `memory-center-core` | Facade crate，重导出 core-logic + 保留原生 IO 实现（SQLite / 文件树存储 + 中文分词 + 高性能缓存） | 大多数 Rust 应用直接依赖此 crate 即可 |

### memory-center-core-logic

纯逻辑核心，定义 `Storage` / `Scorer` / `Migrator` 等 trait 与全部数据模型。`native` feature 启用 jieba 中文分词与 dashmap 并发锁；`wasm` feature 排除这些重依赖。

```rust
use memory_center_core_logic::model::MessageTurn;

let turn: MessageTurn = serde_json::from_str(json_str)?;
// 直接操作数据模型，自行实现 Storage trait
```

主要 API 入口：`model` / `archive` / `retrieve` / `compact` / `score` / `bm25` / `semantic` / `storage::Storage` trait。

线程安全：依赖调用方实现的 `Storage`，crate 自身不持有全局状态。

### memory-center-core

Facade crate，整合 `core-logic` 的纯逻辑 + 原生 IO 实现（`LocalStorage` 文件树 / `SqliteStorage` 连接池 / `CachedStorage` moka 缓存）。绝大多数 Rust 应用应该直接依赖此 crate。

```rust
use memory_center_core::storage::SqliteStorage;

let storage = SqliteStorage::open("./mem_data")?;
let summary = storage.archive("session-001", turns).await?;
let prompt = storage.render_prompt("session-001").await?;
```

主要 API 入口：`storage::LocalStorage` / `storage::SqliteStorage` / `storage::CachedStorage`。

线程安全：`SqliteStorage` 内部 r2d2 连接池 + dashmap per-session 锁，支持多线程并发；`LocalStorage` 推荐每会话独立实例。

## 4. 接口层（4 个）

### memory-center-ffi

C ABI 动态库 + C 头文件，把核心能力暴露为 `memory_center_new` / `memory_center_archive` 等下划线命名函数。MVP 稳定。

- **何时选择**：嵌入 C / C++ / Rust / Go / Swift 等原生应用，需要单二进制零外部依赖
- **代码示例**：

```c
#include "memory_center.h"

MemoryCenterHandle* h = memory_center_new("./mem_data", "sess-001", NULL);
MemoryCenterResult* r = memory_center_archive(h, turns_json);
if (memory_center_is_ok(r)) {
    char* data = memory_center_get_data(r);
    memory_center_free_string(data);
}
memory_center_result_free(r);
memory_center_free(h);
```

- **主要 API 入口**：`memory_center_new` / `memory_center_archive` / `memory_center_retrieve` / `memory_center_render_prompt` / `memory_center_run_compaction` / `memory_center_free`
- **线程安全**：`MemoryCenterHandle` 不保证线程安全，建议每线程独立 handle；返回的字符串必须由调用方用 `memory_center_free_string` 释放

### memory-center-server

Axum HTTP REST API + MCP Streamable HTTP 服务，无状态水平扩展，v2.36 起一个二进制同时提供 REST 与 MCP `/mcp` 端点。

- **何时选择**：远程访问 / 多语言客户端共用 / Web 端 Agent 接入 / 需要水平扩展
- **代码示例**：

```bash
# 启动服务
MEMORY_CENTER_HOST=0.0.0.0 MEMORY_CENTER_PORT=8765 \
  MEMORY_CENTER_ROOT=./data \
  cargo run -p memory-center-server

# 归档
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/archive \
  -H "Content-Type: application/json" \
  -d '{"turns": [...], "project_id": "proj-a"}'
```

- **主要 API 入口**：`POST /api/v1/sessions/{sid}/archive` / `GET /summaries` / `GET /prompt` / `GET /memories/{hook_id}` / `POST /compaction` / `POST /mcp`（Streamable HTTP）
- **线程安全**：服务无状态，每次请求创建独立 `Storage` 实例，天然支持并发与水平扩展；可前置 Nginx / 负载均衡

### memory-center-mcp

MCP Server，支持 stdio（v2.3，本地零配置）与 Streamable HTTP（v2.36，远程多客户端共享）双传输，v2.37 暴露 21 个 tools。

- **何时选择**：Claude Code / Cursor / Trae / Codex CLI 等 AI 编程客户端接入，让 Agent 通过标准协议调用记忆库
- **代码示例**：

```json
// 客户端 MCP 配置（stdio 模式）
{
  "mcpServers": {
    "memory-center": {
      "command": "/path/to/memory-center-mcp",
      "env": { "MEMORY_CENTER_ROOT": "/path/to/data" }
    }
  }
}
```

- **主要 API 入口**：21 个 tools，分 7 类——归档检索（`archive` / `retrieve` / `pre_compress_hook` / `batch_*`）/ 摘要渲染（`summaries` / `prompt` / `get_config`）/ 检索增强（`semantic_search` / `detect_conflicts` / `get_conflicts`）/ 周期任务（`compaction`）/ 预设管理（`preset_*`）/ 项目记忆（`update_project_memory` / `get_project_memory`）/ 规则安装（`install_rules`）
- **线程安全**：stdio 模式单进程串行；Streamable HTTP 模式每次 tool 调用独立 `Storage`，无共享状态

### memory-center-python

Python 原生绑定，基于 PyO3 + maturin 构建，提供 OOP 风格 API 与上下文管理器自动释放。v2.2 稳定。

- **何时选择**：Python 应用 / 脚本 / Jupyter / FastAPI 后端，希望比 ctypes 更原生、更类型安全
- **代码示例**：

```python
from memory_center_python import MemoryCenter

with MemoryCenter("./mem_data", "sess-001", project_id="proj-a") as hp:
    summary = hp.archive([{"user_message": {"text": "你好"}, "llm_message": {"text": "你好！"}}])
    print(summary["hook_id"])
    print(hp.prompt())
    hp.compaction("weekly")
```

- **主要 API 入口**：`MemoryCenter` 类的 `archive` / `retrieve` / `summaries` / `prompt` / `compaction` / `semantic_search` / `detect_conflicts` / `update_project_memory`
- **线程安全**：受 GIL 约束，单实例串行调用；多线程场景请每线程独立实例或使用进程池

## 5. 绑定层（3 个已实现 + 2 个计划中）

| Crate | 状态 | 技术栈 | 何时使用 |
|-------|------|--------|----------|
| `memory-center-wasm` | v2.35 | wasm-bindgen + serde-wasm-bindgen + js-sys | 浏览器扩展 / Edge runtime / 任何支持 WASM 的 JS 运行时 |
| `memory-center-node` | ✅ v2.14 | napi-rs 3.x（异步 Promise API） | Node.js 服务端 / Electron 应用 |
| `memory-center-go` | 计划中（v2.4+） | cgo 调用 C ABI | Go 后端服务 |
| `memory-center-java` | 计划中（v2.4+） | JNA 调用 C ABI | JVM 应用 / Android |

### memory-center-wasm

提供 `MemoryStorage`（纯内存存储）+ `JsStorage`（JS 侧传入 Storage trait 实现，可对接 IndexedDB / OPFS）+ `MemoryCenterCore` JS API。

```js
import init, { MemoryCenterCore, MemoryStorage } from "./pkg/memory_center_wasm.js";

await init();
const storage = new MemoryStorage();
const core = new MemoryCenterCore(storage, "sess-001");
const summary = core.archive(turnsJson);
console.log(summary.hook_id);
```

线程安全：WASM 单线程，无并发问题；异步 IO 通过 `wasm-bindgen-futures` 与 JS Promise 桥接。

### 计划中的绑定

`memory-center-go` / `memory-center-java` 均计划在 v2.4+ 交付。Go 与 Java 通过 cgo / JNA 复用 `memory-center-ffi` 的 C ABI 动态库，无需重写核心逻辑。

## 6. 工具层（9 个）

工具层 crate 主要被 `memory-center-server` / `memory-center-mcp` 内部组合使用，终端用户通常不直接依赖。下列「暴露给终端用户」列标注是否需要用户在 `Cargo.toml` 中显式引用。

| Crate | 一句话定位 | 暴露给终端用户 |
|-------|------------|----------------|
| `memory-center-models` | 型号库——11 个 Agent 预设 + 7 个 Scenario + `ModelVariant` 注册表 + tiktoken 估算 | 是（自定义 Agent / Scenario 时） |
| `memory-center-presets` | 预设配置——`CombinedProfile` 构建 + 场景检测 + Agent 联动 | 是（用 `preset_build` 自定义组合时） |
| `memory-center-agents` | Agent 预设管理——ClaudeCode / Cursor / Trae / Codex 等 11 个内置 Agent | 通常否 |
| `memory-center-scenarios` | 场景管理——coding / writing / research 等 7 个内置场景 + 优先级标签 + 检索策略 | 通常否 |
| `memory-center-llm` | LLM 集成——摘要生成 + 冲突检测 + Embedding + 场景检测（含 HTTP 客户端实现） | 否（被 server / mcp 内部使用） |
| `memory-center-search` | 搜索引擎——BM25 + 语义检索 + `SessionSearchRouter`（session 级搜索路由） | 否（被 server / mcp 内部使用） |
| `memory-center-skills` | 技能管理——内置技能 + 记忆链接 + 技能画像 | 否 |
| `memory-center-windows` | 窗口管理——上下文窗口配置 + 压缩协作策略 | 否 |
| `memory-center-bench` | 性能基准——核心操作 + 后端对比 + 格式对比 + 并发压测（criterion） | 否（仅开发基准测试用） |

工具层 crate 之间也有依赖，例如 `memory-center-presets` 依赖 `memory-center-agents` / `memory-center-scenarios` / `memory-center-models`，`memory-center-server` 同时引用 9 个工具层 crate 中的大部分。

## 7. 依赖关系图

```
                    memory-center-core-logic  (纯逻辑，无 IO)
                              ^
                              |
                    memory-center-core       (Facade，整合 SQLite / 文件树 / 缓存)
                              ^
                              |
        +---------------------+---------------------+---------------------+
        |                     |                     |                     |
memory-center-ffi    memory-center-server   memory-center-mcp    memory-center-python
        |                     |                     |                     |
        |                     +---------------------+                     |
        |                     | (server 内部组合 mcp 的 bootstrap)         |
        |                     v                                           |
        |             memory-center-wasm (依赖 core-logic)                 |
        |                                                                 |
        +-----------------------------------------------------------------+
        (ffi 的 C ABI 也被 memory-center-go / memory-center-java 复用)

工具层依赖：
memory-center-presets → memory-center-agents + memory-center-scenarios + memory-center-models
memory-center-llm     → memory-center-core-logic (trait 定义) + HTTP 客户端实现
memory-center-search  → memory-center-core-logic (BM25 / 语义 trait)
memory-center-server  → 上述全部工具层 crate + memory-center-mcp (Streamable HTTP)
memory-center-mcp     → memory-center-presets + memory-center-llm + memory-center-search
```

关键依赖原则：

- **核心层不反向依赖接口层**：`core-logic` / `core` 不引用任何接口层或工具层 crate
- **工具层不依赖接口层**：`presets` / `llm` / `search` 等只依赖 `core-logic` 的 trait
- **接口层可组合工具层**：`server` / `mcp` 引用工具层以提供完整能力
- **WASM 走 core-logic 捷径**：`memory-center-wasm` 直接依赖 `core-logic`（绕过 `core` 的 SQLite / 文件树重依赖）

## 8. 版本成熟度

| Crate | 状态 | 引入版本 |
|-------|------|----------|
| `memory-center-core-logic` | 稳定 | v2.35 |
| `memory-center-core` | 稳定（MVP） | MVP |
| `memory-center-ffi` | 稳定（MVP） | MVP |
| `memory-center-server` | 稳定 | v2.36 |
| `memory-center-mcp` | 稳定 | v2.37 |
| `memory-center-python` | 稳定 | v2.2 |
| `memory-center-wasm` | 稳定 | v2.35 |
| `memory-center-models` | 稳定 | v2.3 |
| `memory-center-presets` | 稳定 | v2.3 |
| `memory-center-agents` | 稳定 | v2.3 |
| `memory-center-scenarios` | 稳定 | v2.3 |
| `memory-center-llm` | 稳定 | v2.3 |
| `memory-center-search` | 稳定 | v2.3 |
| `memory-center-skills` | 稳定 | v2.3 |
| `memory-center-windows` | 稳定 | v2.3 |
| `memory-center-bench` | 稳定（MVP） | MVP |
| `memory-center-node` | ✅ 已实现 | v2.14 |
| `memory-center-go` | 计划中 | v2.4+ |
| `memory-center-java` | 计划中 | v2.4+ |

> 当前主版本：v2.37。所有「稳定」crate 的 API 在主版本内向后兼容。

## 9. 常见组合

### 组合 1：本地 IDE Agent 接入

最常见组合，适合 Claude Code / Cursor / Trae / Codex CLI 等本地 AI 编程客户端。

```
memory-center-mcp（stdio 传输）
  └─ memory-center-core（Storage 实现）
       └─ memory-center-core-logic（纯逻辑）
```

特点：零配置、单进程、客户端自动发现 21 个 tools。

### 组合 2：远程 Web Agent 接入

适合 Web 端 Agent（如 DeepSeek 网页端）或多客户端共享记忆库。

```
memory-center-server（HTTP REST + MCP Streamable HTTP）
  └─ memory-center-mcp（bootstrap 复用）
       └─ memory-center-core + 全部工具层
```

特点：无状态、水平扩展、可前置负载均衡。

### 组合 3：嵌入式应用

适合 C / C++ / Rust / Go 原生应用嵌入记忆能力。

```
memory-center-ffi（C ABI 动态库）
  └─ memory-center-core
       └─ memory-center-core-logic
```

特点：单二进制零外部依赖、宿主进程内调用、无 IPC 开销。

### 组合 4：Python 应用

适合 Python 脚本、FastAPI 后端、Jupyter Notebook。

```
memory-center-python（PyO3 原生绑定）
  └─ memory-center-core
       └─ memory-center-core-logic
```

特点：OOP 风格 API、上下文管理器自动释放、类型提示完整。

### 组合 5：浏览器扩展

适合 Chrome / Edge 扩展、Edge runtime、Cloudflare Workers。

```
memory-center-wasm（wasm-bindgen）
  └─ memory-center-core-logic（wasm feature）
```

特点：纯客户端运行、`MemoryStorage` 内存存储或 `JsStorage` 对接 IndexedDB。

## 下一步

- [API Reference](API-Reference) —— HTTP REST API 完整端点说明
- [Architecture](Architecture) —— 三级周期与分层架构详解
- [MCP-Integration](MCP-Integration) —— 21 个 MCP tools 详细用法与客户端配置
