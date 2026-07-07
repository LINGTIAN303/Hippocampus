# Architecture

本文档详细描述 MemoryCenter 的整体架构、分层职责、核心机制与数据流，帮助开发者快速建立全局视图。如需快速上手，请先阅读 [Getting Started](Getting-Started)；如需选择具体 crate，参考 [Crate Guide](Crate-Guide)。

## 1. 概览

### 1.1 MemoryCenter 是什么

MemoryCenter 是为 AI Agent 提供的**时序记忆基础设施**（Temporal Memory Infrastructure）。它专注于一件事情：**完整保存对话上下文（非摘要），通过三级周期管理记忆生命周期**。

- **不存向量、不做语义检索、不做 Agent 编排**——这些能力交给向量库与 Agent 框架
- **只做时序归档**——找"之前发生过什么"，而非找"像什么"
- **跨语言可引用**——Rust 核心 + C ABI + HTTP REST + Python + MCP + WASM

### 1.2 核心设计理念

| 理念 | 含义 | 工程体现 |
|------|------|----------|
| **完整归档（非摘要）** | 对话上下文原样冻结为记忆文件，不做压缩/抽取/摘要 | `MemoryFile` 完整保存 `MessageTurn` 数组 |
| **三级周期** | 借鉴大脑记忆系统的分级巩固机制——短期→长期→遗忘 | 天级归档 / 周级去重 / 月级淘汰 |
| **跨语言引用** | 同一组核心能力通过多种接口形态暴露 | C ABI / HTTP / Python / MCP / WASM |
| **可插拔架构** | 所有副作用（存储 / 评分 / 迁移）通过 trait 注入 | `Storage` / `Scorer` / `Migrator` trait |
| **降级友好** | 外部依赖（LLM / Embedder）未配置时自动降级为启发式 | 详见第 9 节 |

### 1.3 命名约定

| 类别 | 约定 | 示例 |
|------|------|------|
| 项目名 | PascalCase | `MemoryCenter` |
| crate 名（带连字符） | kebab-case | `memory-center-core` |
| Rust crate 路径 | snake_case | `memory_center::archive` |
| C ABI 函数名 | snake_case | `memory_center_new` / `memory_center_archive` |
| 环境变量 | UPPER_SNAKE | `MEMORY_CENTER_ROOT` |
| PascalCase 类型 | PascalCase | `MemoryCenterHandle` / `MemoryCenterMcp` / `MemoryCenterCore` |

---

## 2. 三层架构

MemoryCenter 采用严格的三层架构，自下而上为 **Core（核心）→ Interface（接口）→ Bindings（绑定）**。每一层有清晰的职责边界，相邻层之间通过明确定义的契约通信。

### 2.1 架构总览

```
┌──────────────────────────────────────────────────────────────────────────┐
│ Layer 3: Bindings（绑定层 —— 各语言原生 SDK）                              │
│   ① Python 原生绑定 (PyO3, v2.2 ✅, memory-center-python)                  │
│   ② WASM 组件 (v2.35 ✅, memory-center-wasm)                              │
│   ③ Node.js (v2.14 ✅)  ④ Go / Java (v2.4+, 计划中)                      │
├──────────────────────────────────────────────────────────────────────────┤
│ Layer 2: Interface（接口层 —— 跨语言调用入口）                             │
│   ① C ABI 动态库 (MVP ✅, memory-center-ffi)                              │
│   ② Axum HTTP REST (v2.1 ✅, memory-center-server)                        │
│   ③ MCP Server stdio (v2.3 ✅, memory-center-mcp)                         │
│   ④ MCP Streamable HTTP (v2.36 ✅, memory-center-server /mcp 端点)        │
├──────────────────────────────────────────────────────────────────────────┤
│ Layer 1: Core（核心层 —— 纯 Rust 实现）                                    │
│   ┌──────────────────────┬──────────────────────────────────────────┐  │
│   │ memory-center-core   │ Facade crate                              │  │
│   │   （facade）          │   重导出 core-logic + 整合原生 IO 实现     │  │
│   │                      │   （SQLite / 文件树 / 缓存）               │  │
│   ├──────────────────────┼──────────────────────────────────────────┤  │
│   │ memory-center-       │ 纯逻辑 crate                              │  │
│   │   core-logic         │   数据模型 / 归档 / 索引 / 检索 /         │  │
│   │   （pure logic）      │   评分 / BM25 / 语义检索                  │  │
│   │                      │   无 IO 依赖，可编译为 WASM               │  │
│   └──────────────────────┴──────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Layer 1：Core（核心层）

Layer 1 是所有业务逻辑的归宿，由两个 crate 协同组成：

| crate | 角色 | 依赖 IO | 编译目标 |
|-------|------|---------|----------|
| `memory-center-core-logic` | **纯逻辑**：数据模型、归档算法、索引、检索、评分、BM25、语义检索 | 无（所有副作用通过 `Storage` trait 注入） | native + `wasm32-unknown-unknown` |
| `memory-center-core` | **Facade（门面）**：重导出 `core-logic` 全部 API + 保留原生 IO 实现 | 有（`SQLite` / `LocalStorage` / `CachedStorage`） | native only |

**为什么要分离 core-logic 与 core？**

| 关注点 | core-logic | core |
|--------|-----------|------|
| 是否可编译为 WASM | 是 | 否（依赖 tokio / rusqlite / 文件系统） |
| 是否依赖外部 trait 实现 | 是（`Storage` trait 由调用方注入） | 否（自带原生实现） |
| 适用场景 | WASM 组件 / 嵌入式 / 自定义存储后端 | 桌面 / 服务端 / 默认部署 |
| 业务逻辑复用 | 单一源 | 透传 core-logic |

简言之：**core-logic 保证业务逻辑可移植，core 保证原生部署开箱即用**。

### 2.3 Layer 2：Interface（接口层）

接口层将 Core 的异步 Rust API 转换为不同形态的调用入口。四种接口形态对应**同一组核心操作**（archive / retrieve / summaries / prompt / compaction）。

| 接口 | crate | 调用形态 | 状态 | 适合场景 |
|------|-------|----------|------|----------|
| C ABI | `memory-center-ffi` | C 函数 + JSON 字符串 | MVP ✅ | C/C++ 嵌入式 / 宿主进程内调用 |
| HTTP REST | `memory-center-server` | HTTP 端点 + JSON body | v2.1 ✅ | 远程访问 / 多语言共用 |
| MCP stdio | `memory-center-mcp` | MCP tool + JSON 参数 | v2.3 ✅ | AI 编程客户端本地接入 |
| MCP Streamable HTTP | `memory-center-server` 的 `/mcp` 端点 | HTTP + SSE 流式 | v2.36 ✅ | Web 端 Agent / 多客户端共享 |

接口层对比详见下表：

| 维度 | C ABI (FFI) | HTTP REST | Python 原生 | MCP Server |
|------|-------------|-----------|-------------|------------|
| 调用方式 | C 函数 + JSON 字符串 | HTTP 端点 + JSON body | Python 方法 + dict | MCP tool + JSON 参数 |
| 状态 | 有状态（持有 handle） | 无状态（每请求独立） | 有状态（持有实例） | 无状态（每 tool 调用独立） |
| 并发模型 | 单线程，调用方加锁 | 天然并发（tokio 多线程） | GIL 约束，单实例串行 | 单线程 stdio（rmcp） |
| tokio Runtime | `current_thread` | `rt-multi-thread` | `current_thread` | `current_thread` |
| 错误处理 | `MemoryCenterResult` | `{error:{code,message}}` | `PyValueError` | `McpError`（`invalid_params` / `internal_error`） |
| 适合场景 | C/C++ / 嵌入式 | 远程访问 / 多语言 | Python 应用 / 数据科学 | AI 编程客户端（Claude Code / Cursor / Trae / Codex） |

### 2.4 Layer 3：Bindings（绑定层）

绑定层在接口层之上提供各语言的原生 SDK，封装资源管理、类型映射、异常转换等 boilerplate（样板代码）。

| 绑定 | crate | 状态 | 封装能力 |
|------|-------|------|----------|
| Python 原生 | `memory-center-python`（PyO3 0.29 + maturin） | v2.2 ✅ | 上下文管理器 / dict 自动转换 / `PyValueError` 映射 |
| WASM 组件 | `memory-center-wasm`（wasm-bindgen + serde-wasm-bindgen） | v2.35 ✅ | JS Storage 注入 / `MemoryStorage` 兜底 / 浏览器与 Node.js 通用 |
| Node.js | `memory-center-node`（napi-rs 3.x） | ✅ v2.14 | 异步 Promise / TypeScript 类型 |
| Go | `memory-center-go`（cgo） | 🚧 计划中（v2.4+） | 结构体映射 / goroutine 友好 |
| Java | `memory-center-java`（JNA） | 🚧 计划中（v2.4+） | `MemoryCenter` 类 / try-with-resources |

### 2.5 分层原则

| 原则 | 说明 |
|------|------|
| **Layer 1 纯逻辑** | 不依赖 IO（文件系统 / 网络 / 时钟），所有副作用通过 trait 注入 |
| **Layer 2 接口层** | 将 Core 的异步 Rust API 转换为各语言可调用的形式（C ABI / HTTP / MCP / WASM） |
| **Layer 3 绑定层** | 提供各语言的原生 SDK（自动释放 / 类型安全 / 异常映射） |
| **依赖方向** | 严格自上而下：Bindings → Interface → Core → core-logic。core-logic 不依赖任何上层 |

---

## 3. 三级索引周期（核心机制）

三级索引周期是 MemoryCenter 的第一护城河。借鉴大脑记忆系统的分级巩固机制——短期记忆（工作记忆）→ 长期记忆（巩固存储）→ 遗忘淘汰（评分淘汰），将"天 / 周 / 月"映射到工程实现。

### 3.1 周期总览

| 周期 | 操作 | 触发 | 输出 | 索引位置 |
|------|------|------|------|----------|
| **天级（Daily）** | 持续归档 | 会话窗口达 token 阈值 | `MemoryFile`（完整轮次）+ `IndexHook`（钩子） | `daily/index.json` |
| **周级（Weekly）** | 无损去重合并 | 调用 `compaction("weekly")` | 合并后的 `MemoryFile`（原样保留非重复 Turn） | `weekly/index.json` |
| **月级（Monthly）** | 4 维评分淘汰 | 调用 `compaction("monthly")` | 1 个主记忆 + 高价值片段保留 | `monthly/index.json` |

### 3.2 天级（Daily）：持续归档

```
会话进行中                达到 token 阈值              冻结为 MemoryFile
    │                          │                            │
    │  Agent 持续对话           │  软阈值：当前轮次未完成则等待  │
    │  累计 token               │  硬上限：1.5 倍阈值强制截断   │
    │                          │  （标记 truncated=true）     │
    ▼                          ▼                            ▼
                                                            │
                       生成 IndexHook（索引钩子） ◄──────────┘
                            │
                            ▼
                  写入 daily/index.json
                            │
                            ▼
              原始轮次从 LLM 上下文丢弃（释放窗口）
```

**关键点**：
- **非摘要归档**：所有 `MessageTurn` 原样保存，可追溯
- **软 / 硬阈值**：避免在轮次中间截断破坏语义完整性
- **索引钩子分层**：摘要钩子注入 system prompt（轻量），详细钩子通过 tool 检索（按需）

### 3.3 周级（Weekly）：无损去重合并

```
每周触发 weekly_merge
        │
        ▼
  Compactor::weekly_merge
  1. 读取本周 daily 文件（7 天内）
  2. 寒暄剥离（3 条规则）
  3. 去重 + 原样合并（不抽取、不摘要）
  4. 写入 weekly 记忆文件（YYYY-Www.json）
  5. 索引同步合并到 weekly/index.json
  6. 返回 CompactionResult
```

**关键点**：
- **无损**：合并后仍保留所有非重复 Turn，不做语义抽取
- **寒暄剥离**：去除"你好""谢谢"等无信息量轮次
- **去重粒度**：以 `MessageTurn` 为单位，相同内容仅保留首条

### 3.4 月级（Monthly）：4 维评分淘汰

```
每月触发 monthly_evict
        │
        ▼
  Compactor::monthly_evict
  1. 读取本月 weekly 文件（约 4 个）
  2. DefaultScorer 评分（4 维加权）
  3. 选最高分 weekly 为"主记忆"
  4. 其余 weekly 挑高价值 Turn 保留
  5. 写入 monthly 记忆文件（YYYY-MM.json）
  6. 索引同步合并到 monthly/index.json
  7. 返回 CompactionResult
```

**4 维评分维度**：

| 维度 | 权重 | 计算方式 | 说明 |
|------|------|----------|------|
| **时效性（Recency）** | 半衰期 7 天 | 时间衰减函数 | 越新分数越高 |
| **访问频率（Access Frequency）** | 10 次满分 | `access_count` 封顶 | 越常被检索分数越高 |
| **主题相关性（Topic Relevance）** | LLM 评分 | LLM 判断与当前主题相关度 | 需 LLM 配置；未配置时降级为 0 |
| **用户显式标记（User Marking）** | 0-100 | `importance` 字段 | 用户显式标记重要性 |

### 3.5 数据流总览

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Daily 归档     │────▶│   Weekly 合并    │────▶│   Monthly 淘汰  │
│  (持续触发)      │     │  (每周一次)      │     │  (每月一次)      │
└─────────────────┘     └─────────────────┘     └─────────────────┘
        │                       │                       │
        ▼                       ▼                       ▼
   daily/*.json           weekly/*.json           monthly/*.json
   daily/index.json       weekly/index.json       monthly/index.json
        │                       │                       │
        └───────────────────────┴───────────────────────┘
                                │
                                ▼
                    检索时按 daily → weekly → monthly 顺序遍历
                    （近期优先，远期降级查询）
```

---

## 4. 数据流

### 4.1 归档流程

```
Agent 调用方                 Interface 层                  Core 层
     │                              │                          │
     │  archive(handle, turns)      │                          │
     │ ────────────────────────────► │                          │
     │                              │  解析 turns_json          │
     │                              │  ──► Vec<MessageTurn>     │
     │                              │                          │
     │                              │  Archiver::new(...)       │
     │                              │  for turn in turns {      │
     │                              │    archiver.push_turn(...)│
     │                              │  }                        │
     │                              │  ──────────────────────► │
     │                              │                          │ 生成 MemoryFile
     │                              │                          │ Storage::write_memory
     │                              │                          │ 生成 IndexHook
     │                              │                          │ Storage::append_hook
     │                              │                          │  （写入 daily 索引）
     │                              │  ◄────────────────────── │
     │                              │  返回 SummaryView JSON    │
     │  ◄─────────────────────────── │                          │
     │  Result（data = SummaryView） │                          │
```

**归档返回的 `SummaryView` 字段**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `hook_id` | UUID | 索引钩子 ID（检索入口） |
| `memory_file_id` | UUID | 关联的 MemoryFile ID |
| `summary_title` | String | 摘要标题（首条用户消息前 80 字符 或 LLM 生成） |
| `tags` | `Vec<Tag>` | 17 类标签聚合 |
| `archived_at` | DateTime | 归档时间戳 |
| `period` | `ArchivePeriod` | 周期（`Daily` / `Weekly` / `Monthly`） |
| `token_count` | usize | 该记忆文件总 token 数 |

### 4.2 检索流程

MemoryCenter 采用**混合检索（Hybrid Retrieval）**机制，分两阶段：

**阶段 1：摘要钩子注入（被动）**

```
会话开始时
     │
     │  render_prompt(handle)
     │ ──────────────────────► Core
     │                            │
     │                            │ Retriever::render_to_system_prompt
     │                            │  → 遍历 daily/weekly/monthly 索引
     │                            │  → 收集所有 IndexHook 的摘要字段
     │                            │  → 渲染为 Markdown
     │  ◄─────────────────────── │
     │  返回 prompt 文本           │
     │                            │
     ▼
拼接到 LLM system prompt 末尾
（LLM 看到所有可用记忆的标题+标签+时间戳）
```

**阶段 2：详细检索（LLM 主动 tool 调用）**

```
LLM 通过 tool 调用 retrieve(hook_id)
     │
     │  retrieve(handle, hook_id)
     │ ──────────────────────────► Interface
     │                                │
     │                                │ Retriever::retrieve_memory(hook_id)
     │                                │  → 遍历 daily/weekly/monthly 索引
     │                                │  → 找到匹配的 IndexHook
     │                                │  → 读取 hook.memory_file_path
     │                                │  → 返回完整 MemoryFile
     │  ◄──────────────────────────── │
     │  Result（data = MemoryFile JSON）
```

**阶段 3：语义检索（可选）**

```
LLM 通过 tool 调用 semantic_search(query)
     │
     │  semantic_search(handle, query, top_k=5)
     │ ─────────────────────────────────────► Interface
     │                                          │
     │                                          │ SearchEngine::semantic_search
     │                                          │  → Embedder 生成 query 向量
     │                                          │  → 与记忆文件 Embedding 比对
     │                                          │  → BM25 关键词检索兜底
     │                                          │  → 加权合并结果
     │  ◄────────────────────────────────────── │
     │  Result（data = 匹配的 SummaryView 列表）
```

### 4.3 周期任务流程

```
每周触发 weekly_merge                  每月触发 monthly_evict
        │                                      │
        ▼                                      ▼
  Compactor::weekly_merge              Compactor::monthly_evict
  ┌────────────────────────┐           ┌────────────────────────┐
  │ 1. 读取本周 daily 文件  │           │ 1. 读取本月 weekly 文件 │
  │ 2. 寒暄剥离（3 条规则） │           │ 2. DefaultScorer 评分  │
  │ 3. 去重 + 原样合并      │           │    （4 维加权）         │
  │ 4. 写入 weekly 文件     │           │ 3. 选最高分为主记忆     │
  │ 5. 索引合并到 weekly    │           │ 4. 其余挑高价值 Turn   │
  │ 6. 返回 CompactionResult│           │ 5. 写入 monthly 文件   │
  └────────────────────────┘           │ 6. 索引合并到 monthly  │
                                       │ 7. 返回 CompactionResult│
                                       └────────────────────────┘
```

### 4.4 压缩前归档流程（pre_compress_hook）

当客户端（如 Trae / Cursor）即将压缩上下文时，LLM 主动调用 `pre_compress_hook` 一次性归档完整上下文，避免压缩丢失原始内容：

```
LLM 检测到压缩前兆
（客户端提示 / 上下文接近上限 / 用户手动触发）
     │
     │  pre_compress_hook(session_id, full_context, estimated_tokens, task_state_snapshot)
     │ ─────────────────────────────────────────────────────────────────────────────►
     │
     │  双轨处理：
     │  ① raw_context 原样保存（完整字符串备份）
     │  ② 解析为 turns 复用 Archiver 流程（结构化归档）
     │
     │  ◄────────────────────────────────────────────────────────────────────────────
     │  返回归档结果（hook_id + 估算 token 数 + 阈值占比）
```

**与 `archive` 的区别**：

| 维度 | `archive` | `pre_compress_hook` |
|------|-----------|---------------------|
| 触发时机 | 日常归档（达阈值） | 压缩前一次性归档 |
| 输入 | 结构化 turns 数组 | 完整上下文字符串 + 可选 task_state_snapshot |
| 处理方式 | 单轨（结构化 turns） | 双轨（raw_context 原样 + 解析 turns） |
| 核心价值 | 日常记忆生命周期管理 | 即使客户端压缩丢弃原始轮次，MemoryCenter 仍保留完整备份 |

---

## 5. Crate 关系图

MemoryCenter 的 Cargo workspace 包含 **17 个 Rust crate**（native 编译目标），加上 **2 个非 Rust crate**（Go / Java，独立构建系统），共 **19 个 crate 目录**。

### 5.1 分组关系图

```
┌─────────────────────────────────────────────────────────────────────┐
│                       Layer 3: Bindings（绑定层）                    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │
│  │   python     │  │     wasm     │  │  node ✅ / go / java │  │
│  │  (PyO3 0.29) │  │ (wasm-bindgen)│  │  (napi / cgo / JNA)     │  │
│  └──────┬───────┘  └──────┬───────┘  └────────────┬─────────────┘  │
└─────────┼─────────────────┼───────────────────────┼────────────────┘
          │                 │                       │
          ▼                 ▼                       ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Layer 2: Interface（接口层）                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │     ffi      │  │    server    │  │     mcp      │              │
│  │   (C ABI)    │  │ (Axum + MCP) │  │  (rmcp stdio) │              │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘              │
└─────────┼─────────────────┼─────────────────┼───────────────────────┘
          │                 │                 │
          │      ┌──────────┴──────────┐      │
          │      │                     │      │
          ▼      ▼                     ▼      ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Layer 1: Core（核心层）                           │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │              memory-center-core（Facade）                    │   │
│  │   重导出 core-logic + 整合 SQLite / LocalStorage / 缓存      │   │
│  └────────────────────────────┬─────────────────────────────────┘   │
│                               │                                     │
│                               ▼                                     │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │           memory-center-core-logic（纯逻辑）                  │   │
│  │   archive / retrieve / compact / score / bm25 / semantic     │   │
│  │   model / migrator / heuristic / conflict / generate         │   │
│  └──────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
          │                 │                 │             │
          ▼                 ▼                 ▼             ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    工具层（被 Core 依赖）                             │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │
│  │  models  │ │ presets  │ │ agents   │ │scenarios │ │   llm    │  │
│  │ (型号库) │ │ (预设)   │ │ (Agent)  │ │ (场景)   │ │ (LLM)   │  │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘  │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐               │
│  │  search  │ │  skills  │ │ windows  │ │   bench  │               │
│  │ (检索)   │ │ (技能)   │ │ (窗口)   │ │ (基准)   │               │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘               │
└─────────────────────────────────────────────────────────────────────┘
```

### 5.2 Crate 列表

| # | crate | 层级 | 说明 | 状态 |
|---|-------|------|------|------|
| 1 | `memory-center-core-logic` | Core（纯逻辑） | 数据模型 / 归档 / 索引 / 检索 / 评分 / BM25 / 语义检索，可编译为 WASM | ✅ v2.35 |
| 2 | `memory-center-core` | Core（Facade） | 重导出 core-logic + 整合原生 IO（SQLite / 文件树 / 缓存） | ✅ MVP |
| 3 | `memory-center-ffi` | Interface | C ABI 动态库 + C 头文件（`memory_center.h`） | ✅ MVP |
| 4 | `memory-center-server` | Interface | Axum HTTP REST + MCP Streamable HTTP（`/mcp` 端点，无状态） | ✅ v2.36 |
| 5 | `memory-center-mcp` | Interface | MCP Server（stdio 传输，21 个 tools） | ✅ v2.37 |
| 6 | `memory-center-python` | Bindings | Python 原生绑定（PyO3 0.29 + maturin） | ✅ v2.2 |
| 7 | `memory-center-wasm` | Bindings | WASM 组件（wasm-bindgen + `MemoryStorage` + `JsStorage`） | ✅ v2.35 |
| 8 | `memory-center-models` | 工具层 | 型号库（11 个 Agent + 7 个 Scenario + ModelVariant 注册表） | ✅ v2.3 |
| 9 | `memory-center-presets` | 工具层 | 预设配置（`CombinedProfile` 构建 + 场景检测 + Agent 联动） | ✅ v2.3 |
| 10 | `memory-center-agents` | 工具层 | Agent 预设管理（ClaudeCode / Cursor / Trae / Codex 等 11 个） | ✅ v2.3 |
| 11 | `memory-center-scenarios` | 工具层 | 场景管理（coding / writing / research 等 7 个 + 优先级标签） | ✅ v2.3 |
| 12 | `memory-center-llm` | 工具层 | LLM 集成（摘要生成 + 冲突检测 + Embedding + 场景检测） | ✅ v2.3 |
| 13 | `memory-center-search` | 工具层 | 搜索引擎（BM25 + 语义检索 + session 级搜索） | ✅ v2.3 |
| 14 | `memory-center-skills` | 工具层 | 技能管理（内置技能 + 记忆链接 + 技能画像） | ✅ v2.3 |
| 15 | `memory-center-windows` | 工具层 | 窗口管理（上下文窗口配置 + 压缩协作策略） | ✅ v2.3 |
| 16 | `memory-center-bench` | 工具层 | 性能基准（核心操作 + 后端对比 + 格式对比 + 并发压测） | ✅ MVP |
| 17 | `memory-center-node` | Bindings | Node.js 绑定（napi-rs 3.x，异步 Promise） | ✅ v2.14 |
| 18 | `memory-center-go` | Bindings | Go 绑定（cgo，独立 Makefile / go.mod） | 🚧 计划中 |
| 19 | `memory-center-java` | Bindings | Java 绑定（JNA，独立 pom.xml） | 🚧 计划中 |

> 注：17 个 Rust crate 在 `Cargo.toml` 的 `[workspace] members` 中；`memory-center-go` 与 `memory-center-java` 使用各自语言的构建系统（Makefile / Maven），不在 Rust workspace 内。

### 5.3 依赖方向

依赖严格自上而下，**core-logic 不依赖任何上层 crate**：

```
Bindings（python / wasm / node / go / java）
    │
    ▼ 依赖
Interface（ffi / server / mcp）
    │
    ▼ 依赖
Core Facade（core）
    │
    ▼ 重导出
Core 纯逻辑（core-logic）
    │
    ▼ 依赖
工具层（models / presets / agents / scenarios / llm / search / skills / windows）
```

---

## 6. 存储后端

MemoryCenter 的存储层采用可插拔 trait 设计，所有副作用通过 `Storage` trait 注入。默认提供多种后端实现，覆盖不同部署场景。

### 6.1 Storage trait 与实现

| 实现 | crate | 适用场景 | 特性 |
|------|-------|----------|------|
| `LocalStorage` | `memory-center-core` | 桌面 / 服务端默认 | 文件树存储 + `RwLock` 单写多读 + 原子写入（temp + rename） |
| `SqliteStorage` | `memory-center-core` | 生产环境 / 高并发 | SQLite WAL 模式 + `r2d2` 连接池 + 内置向量搜索 |
| `CachedStorage` | `memory-center-core` | 高频读取场景 | `moka` 异步缓存 + 透明代理底层 Storage |
| `MemoryStorage` | `memory-center-core-logic` / `memory-center-wasm` | 测试 / demo / WASM 兜底 | 纯内存 `HashMap`，进程退出即丢失 |
| `JsStorage` | `memory-center-wasm` | WASM 组件 | JS 调用方实现 `read` / `write` / `list`，Rust 通过 `js_sys::Function` 调用 |

### 6.2 存储布局

所有原生后端（`LocalStorage` / `SqliteStorage`）共享同一套存储布局：

```
<root_path>/                          # 由 MEMORY_CENTER_ROOT 指定
└── sessions/
    └── <session_id>/                 # 会话级隔离
        └── [projects/<project_id>/]  # 可选，project_id 存在时
            ├── daily/
            │   ├── index.json        # IndexDocument（钩子集合）
            │   ├── 2026-07-02_143052_123.json   # MemoryFile
            │   └── 2026-07-02_150000_456.json
            ├── weekly/
            │   ├── index.json
            │   └── 2026-W27.json     # ISO 周编号
            └── monthly/
                ├── index.json
                └── 2026-07.json
```

### 6.3 文件命名规则

| 周期 | 格式 | 示例 | 说明 |
|------|------|------|------|
| Daily | `YYYY-MM-DD_HHMMSS_mmm.json` | `2026-07-02_143052_123.json` | 毫秒级时间戳，避免并发冲突 |
| Weekly | `YYYY-Www.json` | `2026-W27.json` | ISO 8601 周编号 |
| Monthly | `YYYY-MM.json` | `2026-07.json` | 年月 |

### 6.4 序列化格式

| 格式 | 启用方式 | 适用场景 |
|------|----------|----------|
| JSON | 默认 | 可调试优先，开发期推荐 |
| MessagePack | 配置开关 | 生产环境，体积更小、解析更快 |

---

## 7. 可插拔架构

MemoryCenter 的核心设计原则之一是**所有副作用通过 trait 注入**。这意味着存储、评分、迁移等关键能力均可替换实现，无需修改业务逻辑。

### 7.1 可插拔 trait 列表

| Trait | 默认实现 | 替换场景 | 替换方式 |
|-------|----------|----------|----------|
| `Storage` | `LocalStorage` / `SqliteStorage` | S3 / Redis / PostgreSQL / 自定义云存储 | 实现 trait 后注入 `Archiver` / `Retriever` / `Compactor` |
| `Scorer` | `DefaultScorer`（4 维启发式） | LLM 评分 / 自定义加权策略 | 实现 trait 后注入 `Compactor::monthly_evict` |
| `Migrator` | （v2 默认实现） | Schema 升级 / 历史数据迁移 | 实现 trait 后调用迁移入口 |
| `Embedder` | （需配置 API） | 本地 Embedding 模型 / 其他云服务 | 实现 trait 后注入 `SearchEngine` |
| `SummaryGenerator` | 启发式摘要（首条消息前 80 字符） | LLM 摘要生成 / 自定义模板 | 实现 trait 后注入 `Archiver` |
| `ConflictDetector` | 启发式纯算法（三维度检测） | LLM 冲突检测 / 自定义规则 | 实现 trait 后注入冲突检测入口 |

### 7.2 扩展示例：自定义 Storage

```rust
use memory_center_core::storage::Storage;
use memory_center_core::model::MemoryFile;

// 1. 实现 Storage trait（这里以 S3 为例，伪代码）
struct S3Storage {
    bucket: String,
    client: S3Client,
}

#[async_trait::async_trait]
impl Storage for S3Storage {
    async fn write_memory(&self, file: &MemoryFile) -> Result<(), Error> {
        let key = format!("memories/{}.json", file.id);
        let body = serde_json::to_string(file)?;
        self.client.put_object(&key, body).await?;
        Ok(())
    }
    // ... 其他方法
}

// 2. 注入到 Archiver
let storage = Arc::new(S3Storage { /* ... */ });
let archiver = Archiver::new(storage.clone(), session_id, project_id);
// 后续调用 archiver.push_turn(...) 即可将记忆写入 S3
```

### 7.3 扩展点总结

| 扩展点 | 影响范围 | 复杂度 |
|--------|----------|--------|
| 替换 `Storage` | 记忆文件持久化位置 | 中（需实现完整 trait） |
| 替换 `Scorer` | 月级淘汰评分策略 | 低（单一函数） |
| 替换 `Embedder` | 语义检索的向量来源 | 低（调用外部 API） |
| 替换 `SummaryGenerator` | 摘要钩子的标题生成 | 低（可调用 LLM） |
| 替换 `ConflictDetector` | 冲突检测算法 | 中（需理解三维度语义） |

---

## 8. 线程安全与并发模型

不同接口层有不同的并发特性，开发者需根据场景选择合适的接入方式。

### 8.1 各层并发模型对比

| 层级 | 组件 | 线程安全 | tokio Runtime | 并发能力 |
|------|------|----------|---------------|----------|
| Layer 1 | `LocalStorage` | 单写多读（`RwLock`） | 不需要 | 读无锁，写串行化 |
| Layer 1 | `SqliteStorage` | WAL 模式 + 连接池 | `spawn_blocking` | 高并发读写 |
| Layer 2 | `MemoryCenterHandle` (FFI) | 不保证线程安全 | `current_thread` | 单线程，调用方加锁 |
| Layer 2 | HTTP Server | 无状态，天然并发 | `rt-multi-thread` | 水平扩展 |
| Layer 2 | MCP Server (stdio) | 无状态，每 tool 独立 | `current_thread` | 单线程 stdio |
| Layer 3 | Python 绑定 | 受 GIL 约束 | `current_thread` | 单实例串行 |
| Layer 3 | WASM 组件 | 单线程（WASM 限制） | 不需要 | 单线程 |

### 8.2 Layer 1（Core）并发细节

- **单写多读**：`LocalStorage` 内部 `RwLock`，读操作无锁，写操作串行化
- **原子写入**：temp 文件 + rename（防崩溃损坏）
- **读-改-写**：索引更新采用 `read → modify → write back` 模式
- **细粒度锁**：`dashmap` 提供 per session / per project 的细粒度并发锁

### 8.3 Layer 2 - FFI（C ABI）并发建议

```c
/* 错误做法：多线程共享 handle */
MemoryCenterHandle* h = memory_center_new(...);
/* thread_a: memory_center_archive(h, ...); */
/* thread_b: memory_center_archive(h, ...);  // 可能数据竞争 */

/* 正确做法 1：每线程独立 handle */
MemoryCenterHandle* h_a = memory_center_new(..., "session-001", ...);
MemoryCenterHandle* h_b = memory_center_new(..., "session-001", ...);

/* 正确做法 2：调用方加锁（如 pthread_mutex） */
pthread_mutex_lock(&lock);
memory_center_archive(h, ...);
pthread_mutex_unlock(&lock);
```

### 8.4 Layer 2 - HTTP Server 并发特性

- **无状态设计**：每次请求创建独立 `Storage`，无内存会话池
- **tokio Runtime**：`rt-multi-thread`（支持并发请求）
- **天然水平扩展**：无状态 + 文件存储，可多实例部署
- **SQLite 连接池**：`r2d2` + WAL 模式，支持高并发读写

### 8.5 Layer 3 - Python 绑定并发特性

- **GIL 约束**：单实例串行调用（PyO3 同步 API）
- **内部 tokio Runtime**：`current_thread`（与 FFI 一致）
- **上下文管理器**：`with MemoryCenter(...) as hp:` 自动释放资源
- **建议**：多会话用多实例（每会话一个 `MemoryCenter` 对象）

### 8.6 Layer 2 - MCP Server 并发特性

- **无状态设计**：每次 tool 调用创建独立 `Storage`，无共享状态
- **stdio 传输**：rmcp 单线程 stdio 模型，被客户端作为子进程拉起
- **会话隔离**：通过 tool 参数 `session_id` / `project_id` 区分不同会话
- **Streamable HTTP 模式**：通过 `/mcp` 端点支持多客户端共享，复用 Axum 的 `rt-multi-thread`

---

## 9. 降级机制

MemoryCenter 在外部依赖（LLM / Embedder）未配置时自动降级为启发式实现，保证核心功能可用。这是"零外部依赖也能跑"的关键设计。

### 9.1 降级场景

| 未配置的外部依赖 | 降级行为 | 影响范围 | 启用方式 |
|------------------|----------|----------|----------|
| **LLM 摘要生成器** | 启发式摘要（首条消息前 80 字符） | `SummaryView.summary_title` 字段 | 配置 LLM API 后自动启用 LLM 摘要 |
| **Embedder API** | 仅关键词检索（BM25） | `semantic_search` 退化为关键词检索 | 配置 Embedding API 后启用语义检索 |
| **LLM 冲突检测器** | 启发式纯算法（三维度检测） | `detect_conflicts` 精度降低 | 配置 LLM API 后启用 LLM 检测 |
| **Agent 客户端未识别** | 不注入 `usage_protocol` | LLM 需依赖 `AGENTS.md` 主动调用 | 通过 `preset_list_agents` 显式指定 |

### 9.2 降级检测逻辑

```
启动时检查环境变量 / 配置文件
        │
        ├── LLM API 配置存在？
        │       ├── 是 → 启用 LLM 摘要 / 冲突检测
        │       └── 否 → 降级为启发式摘要 / 纯算法检测
        │
        ├── Embedder API 配置存在？
        │       ├── 是 → 启用语义检索（BM25 + 向量混合）
        │       └── 否 → 降级为纯 BM25 关键词检索
        │
        └── Agent 客户端识别？
                ├── 是 → 注入对应 usage_protocol
                └── 否 → 依赖 AGENTS.md 规则文件
```

### 9.3 降级不影响的核心能力

无论是否配置外部依赖，以下核心能力**始终可用**：

| 能力 | 说明 |
|------|------|
| 完整上下文归档 | `archive` / `pre_compress_hook` 不依赖 LLM |
| 索引钩子检索 | `retrieve` / `summaries` / `prompt` 不依赖 LLM |
| 周期任务 | `weekly_merge` 不依赖 LLM；`monthly_evict` 降级为 3 维评分（无主题相关性） |
| 冲突检测 | 启发式三维度检测仍可工作（精度略低） |
| BM25 关键词检索 | 纯 Rust 实现（`jieba-rs` 中文分词），无外部依赖 |

### 9.4 配置外部依赖

启用完整能力需配置以下环境变量（任选其一）：

```bash
# LLM API（用于摘要生成 + 冲突检测）
export MEMORY_CENTER_LLM_API_KEY="sk-..."
export MEMORY_CENTER_LLM_BASE_URL="https://api.openai.com/v1"
export MEMORY_CENTER_LLM_MODEL="gpt-4o-mini"

# Embedder API（用于语义检索）
export MEMORY_CENTER_EMBEDDER_API_KEY="sk-..."
export MEMORY_CENTER_EMBEDDER_BASE_URL="https://api.openai.com/v1"
export MEMORY_CENTER_EMBEDDER_MODEL="text-embedding-3-small"
```

> 未配置时，MemoryCenter 启动时会输出 `WARN` 日志提示降级状态，但不影响核心功能。

---

## 下一步

- [Crate Guide](Crate-Guide) —— 选择合适的 Crate（按使用场景对比 19 个 crate）
- [MCP Integration](MCP-Integration) —— 接入 Claude Code / Cursor / Trae / Codex CLI
- [Deployment](Deployment) —— 部署到生产环境（含 Streamable HTTP 多客户端共享）
