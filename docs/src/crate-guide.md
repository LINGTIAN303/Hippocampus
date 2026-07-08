# Crate 选择指南

> 本章节是 [GitHub Wiki: Crate Guide](https://github.com/LINGTIAN303/MemoryCenter/wiki/Crate-Guide) 的镜像。

## Crate 矩阵

| Crate | 说明 | 状态 |
|-------|------|------|
| `memory-center-core-logic` | 纯逻辑核心（数据模型 / 归档 / 索引 / 检索 / 评分 / BM25 / 语义检索），可编译为 WASM | ✅ v2.35 |
| `memory-center-core` | Facade crate，重导出 core-logic + 保留原生 IO 实现（SQLite / 文件树存储） | ✅ MVP |
| `memory-center-ffi` | C ABI 动态库 + C 头文件 | ✅ MVP |
| `memory-center-server` | Axum HTTP REST API + MCP Streamable HTTP 服务（无状态，水平扩展） | ✅ v2.36 |
| `memory-center-python` | Python 原生绑定（PyO3 + maturin） | ✅ v2.2 |
| `memory-center-mcp` | MCP Server（stdio + Streamable HTTP，21 个 tools） | ✅ v2.37 |
| `memory-center-wasm` | WASM 组件（wasm-bindgen + MemoryStorage + JsStorage） | ✅ v2.35 |
| `memory-center-models` | 型号库（11 个 Agent + 7 个 Scenario + ModelVariant 注册表） | ✅ v2.3 |
| `memory-center-presets` | 预设配置（CombinedProfile 构建 + 场景检测 + Agent 联动） | ✅ v2.3 |
| `memory-center-agents` | Agent 预设管理（ClaudeCode / Cursor / Trae / Codex 等 11 个） | ✅ v2.3 |
| `memory-center-scenarios` | 场景管理（coding / writing / research 等 7 个 + 优先级标签） | ✅ v2.3 |
| `memory-center-llm` | LLM 集成（摘要生成 + 冲突检测 + Embedding + 场景检测） | ✅ v2.3 |
| `memory-center-search` | 搜索引擎（BM25 + 语义检索 + session 级搜索） | ✅ v2.3 |
| `memory-center-skills` | 技能管理（内置技能 + 记忆链接 + 技能画像） | ✅ v2.3 |
| `memory-center-windows` | 窗口管理（上下文窗口配置 + 压缩协作策略） | ✅ v2.3 |
| `memory-center-bench` | 性能基准（核心操作 + 后端对比 + 格式对比 + 并发压测） | ✅ MVP |
| `memory-center-node` | Node.js 绑定（napi-rs 3.x，异步 Promise API） | ✅ v2.14 |
| `memory-center-go` | Go 绑定（cgo，v2.4+） | 🚧 计划中 |
| `memory-center-java` | Java 绑定（JNA，v2.4+） | 🚧 计划中 |

## 选择决策树

```
你的场景是什么？
├─ AI 编程客户端（Claude Code / Cursor / Trae）
│   └─ memory-center-mcp（stdio 模式）
├─ 远程 Agent / Web 端
│   └─ memory-center-server（MCP Streamable HTTP + REST API）
├─ Python 项目
│   └─ memory-center-python（PyO3 原生绑定）
├─ Node.js 项目
│   └─ memory-center-node（napi-rs）
├─ C/C++ 项目
│   └─ memory-center-ffi（C ABI 动态库）
├─ Rust 项目
│   └─ memory-center-core（直接依赖 facade crate）
├─ 浏览器 / Edge
│   └─ memory-center-wasm（wasm-bindgen）
└─ 其他语言
    └─ 通过 HTTP REST API 接入
```

## 依赖关系

- **`core-logic`**：纯逻辑，无 IO 依赖，可编译为 WASM
- **`core`**：依赖 `core-logic`，添加原生 IO（SQLite / 文件树）
- **`ffi` / `python` / `node` / `wasm`**：依赖 `core`（或 `core-logic` for WASM）
- **`server` / `mcp`**：依赖 `core` + `llm` + `search` + `presets` 等
- **业务 crate 不可反向依赖入口 crate**

## 详细文档

完整指南见 [Wiki: Crate Guide](https://github.com/LINGTIAN303/MemoryCenter/wiki/Crate-Guide)。
