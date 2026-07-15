# Cooperative 协作模式设计文档

> P8 · windows crate 扩展 · v2.53 设计稿
>
> 状态：**设计阶段（未实现）** · 评审通过后进入实现阶段

## 1. 背景与动机

### 1.1 Independent 模式的局限性

当前 MemoryCenter 的 `CooperationMode::Independent` 是 MVP 默认模式，工作流为：

```
Agent 工具独立管理上下文
    ↓  接近窗口上限时
Agent 工具触发压缩（如 Claude Code /compact）
    ↓  丢弃旧轮次时
Agent 工具调用 MemoryCenter archive 归档被丢弃的内容
    ↓
MemoryCenter 被动接收归档
```

**核心痛点**：

| 痛点 | 描述 | 影响 |
|------|------|------|
| **归档时机被动** | MemoryCenter 无法预知压缩事件，只能在 Agent 触发后被动接收 | 归档内容可能不完整（Agent 已丢弃部分轮次） |
| **无保留建议** | MemoryCenter 无法反向建议 Agent 保留哪些上下文 | 重要决策、关键代码片段可能被压缩丢弃 |
| **单向通信** | 仅 Agent → MemoryCenter，无反向通道 | MemoryCenter 的记忆库无法主动服务于上下文管理 |
| **Context Rot** | 长对话中上下文质量随 token 增长而衰减（[Anthropic context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)） | 模型注意力预算耗尽，信息检索精度下降 |

### 1.2 业界趋势

业界正从"被动归档"向"主动协作管理"演进：

- **Anthropic**（2026）：提出 context engineering 三策略 —— Compaction（压缩）、Structured note-taking（结构化笔记）、Sub-agent（子 Agent 架构）。Compaction 的核心在于"选择保留什么 vs 丢弃什么"，这正是 Cooperative 模式要解决的问题。
- **MemGPT**：三层内存架构 —— LLM context window（最快）→ Core memory（KB 级持久身份）→ Archival memory（GB 级海量历史）。Agent 主动在层间迁移信息，而非被动等待窗口溢出。
- **Redis**（2026）：context window overflow 五策略 —— smart chunking、semantic caching、session management 等，强调"软溢出"（context rot）比"硬限制"更需关注。

### 1.3 设计动机

Cooperative 模式的核心价值：**让 MemoryCenter 从被动归档者升级为主动协作管理者**，在 Agent 压缩上下文前提供保留建议，减少关键信息丢失。

## 2. 现状分析

### 2.1 Independent 模式工作流

```
┌──────────────┐                    ┌──────────────┐
│  Agent 工具   │                    │ MemoryCenter │
│ (Claude Code) │                    │   (MCP/HTTP) │
└──────┬───────┘                    └──────┬───────┘
       │                                   │
       │  对话进行中...                     │
       │  (token 持续增长)                  │
       │                                   │
       │  接近窗口上限                      │
       │  (如 180K/200K)                   │
       │                                   │
       │  触发压缩                          │
       │  (如 /compact)                    │
       │                                   │
       │  ─── archive(turns) ────────────→ │
       │                                   │  归档 turns
       │                                   │  生成 IndexHook
       │                                   │  估算 token
       │  ←──── archive_result ────────── │
       │                                   │
       │  丢弃旧轮次                        │
       │  保留摘要 + 最近 N 轮              │
       │                                   │
       │  继续对话...                       │
```

**关键观察**：归档发生在压缩**之后**，MemoryCenter 无法干预保留决策。

### 2.2 当前代码结构

| 文件 | 内容 | Cooperative 相关 |
|------|------|------------------|
| `cooperation.rs` | `CooperationMode` enum（Independent/Cooperative） | Cooperative 变体存在但 `is_supported()` 返回 false |
| `window_profile.rs` | `WindowProfile` struct + `validate()` | validate() 主动拒绝 Cooperative |
| `compression.rs` | `CompressionScheme` enum（6 变体） | 无变化（压缩方式与协作模式正交） |
| `lib.rs` | crate 级文档注释 | 标注 "Cooperative（v2）" |

### 2.3 消费方

| 消费方 | 使用方式 | Cooperative 影响 |
|--------|----------|-------------------|
| `memory-center-presets` | `linkage.rs` 推导 WindowProfile + `builder.rs` 校验 | 需支持 Cooperative 预设 |
| `memory-center-python` | `window_scheme_from_str()` 字符串解析 | 需新增 "cooperative" 解析 |
| `memory-center-archive-core` | `ArchiveEngine` 归档核心 | 需新增 Cooperative 处理逻辑 |
| `memory-center-mcp` | MCP 工具暴露 | 需新增 pre_compress_hint 工具 |
| `memory-center-server` | HTTP API | 需新增 /api/v1/cooperative/* 端点 |

## 3. 业界参考

### 3.1 Anthropic Context Engineering

Anthropic 提出的三种长时任务上下文管理策略，与 Cooperative 模式的设计高度相关：

| 策略 | 描述 | Cooperative 借鉴点 |
|------|------|---------------------|
| **Compaction** | 对话接近窗口限制时摘要内容并重新启动 | Cooperative 的 pre_compress_hint 让 MemoryCenter 在压缩前参与保留决策 |
| **Structured note-taking** | Agent 定期将笔记写入上下文窗口外记忆 | MemoryCenter 的 standalone/linked memory 已实现此能力（P7 Phase 3） |
| **Sub-agent** | 专门子 Agent 处理聚焦任务，返回浓缩摘要 | 不在 Cooperative 范围内（属于 Agent 编排层） |

**关键启示**：Compaction 的核心挑战是"选择保留什么 vs 丢弃什么"，Anthropic 建议"先最大化召回，再迭代提升精度"。Cooperative 模式的保留建议应遵循此原则 —— 宁可多建议保留，不可遗漏关键信息。

### 3.2 MemGPT 三层内存架构

```
┌─────────────────────────────────┐
│  LLM Context Window             │  ← 最小、最快
│  (系统提示 + 工具 + 消息历史)    │
└────────────┬────────────────────┘
             │  溢出时迁移
             ↓
┌─────────────────────────────────┐
│  Core Memory (KB 级)             │  ← 持久身份信息
│  (用户画像、项目状态、关键决策)   │
└────────────┬────────────────────┘
             │  长期归档
             ↓
┌─────────────────────────────────┐
│  Archival Memory (GB 级+)        │  ← 海量历史
│  (完整对话归档、搜索索引)         │
└─────────────────────────────────┘
```

**与 MemoryCenter 的对应**：

| MemGPT 层 | MemoryCenter 对应 | 当前实现 |
|-----------|-------------------|----------|
| LLM Context Window | Agent 工具的上下文窗口 | Agent 管理，MemoryCenter 不直接参与 |
| Core Memory | `project_memory.md` + standalone/linked memory | ✅ P7 Phase 3 已实现 |
| Archival Memory | archive 存储的 IndexHook + 搜索索引 | ✅ 已实现 |

**关键启示**：MemGPT 的 Agent 主动在层间迁移信息（如 `core_memory_append` 工具），而非被动等待。Cooperative 模式应让 MemoryCenter 主动建议 Agent 将重要信息写入 Core Memory（即 standalone/linked memory）。

### 3.3 Redis Context Window Overflow 五策略

| 策略 | 描述 | Cooperative 借鉴点 |
|------|------|---------------------|
| Smart chunking | 将大文档分块按需加载 | 不在 Cooperative 范围内 |
| Semantic caching | 语义缓存减少重复 LLM 调用 | 保留建议可缓存，避免重复检索 |
| Session management | 会话级上下文管理 | Cooperative 会话状态管理 |
| Summarization | 摘要压缩 | archive-core 已实现（SummaryGenerator） |
| Context pruning | 上下文修剪 | Cooperative 的保留建议即反向修剪 |

**关键启示**：Redis 强调"软溢出"（context rot）比"硬限制"更需关注。Cooperative 模式应在 Agent 感知到 context rot 时（而非仅窗口快满时）就触发协作。

## 4. 设计目标与非目标

### 4.1 设计目标

1. **双向通信**：Agent ↔ MemoryCenter 双向同步通信，MemoryCenter 能在压缩前提供保留建议
2. **主动保留建议**：基于语义检索，从已归档记忆中找出与当前上下文最相关的内容
3. **渐进式压缩**：分阶段压缩（先修剪工具输出 → 再摘要历史 → 最后丢弃低价值轮次），每阶段都可协作
4. **向后兼容**：不破坏 Independent 模式，Cooperative 失败时自动降级为 Independent
5. **无新外部依赖**：复用现有 Embedder API + BM25 检索能力，不引入新依赖

### 4.2 非目标

| 非目标 | 原因 |
|--------|------|
| 替代 Agent 的压缩能力 | 压缩仍由 Agent 工具执行，MemoryCenter 只提供保留建议 |
| 多 Agent 协作编排 | 多 Agent 共享记忆是另一个维度（见 scenarios/01-scenario-design.md 场景三），不在 P8 范围 |
| 实时上下文监控 | 不做 Agent 上下文的实时监控/push，仅在 Agent 主动请求时协作 |
| LLM 生成保留建议 | 保留策略用语义检索（已确认），不调用 LLM 生成建议 |
| WebSocket 长连接 | 通信方式用同步请求-响应（已确认），不维护长连接 |

## 5. 核心概念

### 5.1 概念总览

```
┌─────────────────────────────────────────────────────────────┐
│                    CooperativeSession                        │
│                                                             │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────────┐    │
│  │ Compression  │  │  Retention   │  │   Context       │    │
│  │ Event        │  │  Suggestion  │  │   Snapshot      │    │
│  │ (压缩事件)   │  │  (保留建议)  │  │   (上下文快照)   │    │
│  └─────────────┘  └──────────────┘  └─────────────────┘    │
│                                                             │
│  状态：Idle → Notified → Analyzing → Suggesting →            │
│         Compressing → Archived → Idle                       │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 概念定义

#### CooperativeSession（协作会话）

一个 Agent 与 MemoryCenter 之间的协作会话，维护协作状态和上下文快照。

- **生命周期**：从 Agent 首次请求协作到会话结束（Agent 断开或显式关闭）
- **状态**：有限状态机（见第 7 章）
- **隔离**：每个 Agent 会话独立，通过 `session_id` 隔离

#### CompressionEvent（压缩事件）

Agent 通知 MemoryCenter 即将发生压缩的事件。

- **触发方**：Agent（如 Claude Code 即将执行 /compact）
- **内容**：当前 token 估算、窗口上限、压缩方式、待压缩轮次范围
- **时机**：压缩**前**（让 MemoryCenter 有时间生成保留建议）

#### RetentionSuggestion（保留建议）

MemoryCenter 反向提供给 Agent 的保留建议。

- **生成方**：MemoryCenter（基于语义检索）
- **内容**：建议保留的轮次 ID 列表 + 保留原因 + 优先级
- **依据**：当前上下文快照 + 已归档记忆的语义检索
- **执行方**：Agent（建议非强制，Agent 最终决定是否采纳）

#### ContextSnapshot（上下文快照）

Agent 提供给 MemoryCenter 的当前上下文摘要。

- **内容**：最近 N 轮的关键信息（任务状态、涉及的文件、工具调用摘要）
- **用途**：作为语义检索的 query，从已归档记忆中检索相关内容
- **隐私**：可配置脱敏级别（完整内容 / 摘要 / 仅关键词）

## 6. 双向通信协议设计

### 6.1 通信模式：同步请求-响应

采用同步请求-响应模式，与现有 MCP/REST 架构一致，无状态维护负担。

```
┌──────────────┐                    ┌──────────────┐
│  Agent 工具   │                    │ MemoryCenter │
│ (Claude Code) │                    │   (MCP/HTTP) │
└──────┬───────┘                    └──────┬───────┘
       │                                   │
       │  1. pre_compress_hint              │
       │  (压缩前通知 + 上下文快照)          │
       │  ───────────────────────────────→ │
       │                                   │
       │                                   │  2. 语义检索
       │                                   │  (从已归档记忆中
       │                                   │   检索相关内容)
       │                                   │
       │                                   │  3. 生成保留建议
       │                                   │  (RetentionSuggestion)
       │                                   │
       │  4. RetentionSuggestion            │
       │  (保留建议 + 优先级)                │
       │  ←─────────────────────────────── │
       │                                   │
       │  5. Agent 执行压缩                 │
       │  (采纳/部分采纳/忽略建议)           │
       │                                   │
       │  6. post_compress_ack              │
       │  (压缩后确认 + 归档)                │
       │  ───────────────────────────────→ │
       │                                   │
       │                                   │  7. 归档被压缩内容
       │                                   │  (复用现有 archive 链路)
       │                                   │
       │  8. archive_result                │
       │  ←─────────────────────────────── │
       │                                   │
```

### 6.2 三个核心交互

#### 交互 1：pre_compress_hint（压缩前通知）

**方向**：Agent → MemoryCenter

**时机**：Agent 决定执行压缩**之前**（如 Claude Code 用户输入 /compact，或自动压缩阈值触发）

**请求内容**：

```json
{
  "session_id": "trae-memory-center-20260715",
  "event_type": "pre_compress",
  "current_tokens": 175000,
  "token_threshold": 200000,
  "compression_scheme": "ClaudeCodeCompact",
  "context_snapshot": {
    "current_task": "P8 Cooperative 设计文档编写",
    "recent_turns_summary": "用户确认三个设计决策，正在编写设计文档...",
    "key_files": ["docs/cooperative-design.md", "crates/memory-center-windows/src/cooperation.rs"],
    "tool_calls_summary": ["Read cooperation.rs", "WebSearch context engineering", "Write cooperative-design.md"]
  },
  "turns_to_compress": [
    {"turn_id": "turn-001", "text_preview": "探索 windows crate...", "token_count": 1500},
    {"turn_id": "turn-002", "text_preview": "搜索外部资料...", "token_count": 2000}
  ]
}
```

**字段说明**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `session_id` | String | 是 | 会话 ID |
| `event_type` | String | 是 | 固定 "pre_compress" |
| `current_tokens` | usize | 是 | 当前 token 估算 |
| `token_threshold` | usize | 是 | 窗口上限 |
| `compression_scheme` | String | 是 | 压缩方式（对应 CompressionScheme） |
| `context_snapshot` | Object | 是 | 上下文快照 |
| `context_snapshot.current_task` | String | 否 | 当前任务描述 |
| `context_snapshot.recent_turns_summary` | String | 否 | 最近轮次摘要 |
| `context_snapshot.key_files` | Vec\<String\> | 否 | 涉及的文件列表 |
| `context_snapshot.tool_calls_summary` | Vec\<String\> | 否 | 工具调用摘要 |
| `turns_to_compress` | Vec\<TurnPreview\> | 否 | 待压缩轮次预览 |

#### 交互 2：RetentionSuggestion（保留建议）

**方向**：MemoryCenter → Agent（作为交互 1 的响应）

**响应内容**：

```json
{
  "session_id": "trae-memory-center-20260715",
  "suggestion_id": "sugg-20260715-001",
  "retain_turns": [
    {
      "turn_id": "turn-001",
      "priority": "high",
      "reason": "包含 windows crate 架构分析，与当前设计文档直接相关",
      "related_memories": ["hook_id:17f1d7ac", "hook_id:175e30ff"]
    },
    {
      "turn_id": "turn-003",
      "priority": "medium",
      "reason": "包含 Anthropic context engineering 参考，后续章节可能引用",
      "related_memories": []
    }
  ],
  "prune_hints": [
    {
      "target": "tool_results",
      "reason": "WebSearch/WebFetch 的原始结果可安全修剪，摘要在 context_snapshot 中已保留"
    },
    {
      "target": "turn-004",
      "reason": "探索性搜索，未产生关键发现"
    }
  ],
  "inject_memories": [
    {
      "hook_id": "175e30ff",
      "reason": "P9 sentencepiece 集成完成记录，包含架构决策可能需要参考",
      "inject_strategy": "summary"
    }
  ]
}
```

**字段说明**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `retain_turns` | Vec\<RetainItem\> | 建议保留的轮次列表 |
| `retain_turns[].turn_id` | String | 轮次 ID |
| `retain_turns[].priority` | String | 优先级（high/medium/low） |
| `retain_turns[].reason` | String | 保留原因 |
| `retain_turns[].related_memories` | Vec\<String\> | 关联的已归档记忆 hook_id |
| `prune_hints` | Vec\<PruneHint\> | 修剪建议（可安全丢弃的内容） |
| `inject_memories` | Vec\<InjectItem\> | 建议注入上下文的已归档记忆 |

**三段式建议策略**：

1. **retain_turns**（保留）：哪些轮次建议保留，附优先级和原因
2. **prune_hints**（修剪）：哪些内容可安全修剪（如工具原始输出）
3. **inject_memories**（注入）：哪些已归档记忆建议注入上下文

#### 交互 3：post_compress_ack（压缩后确认 + 归档）

**方向**：Agent → MemoryCenter

**时机**：Agent 执行压缩**之后**

**请求内容**：

```json
{
  "session_id": "trae-memory-center-20260715",
  "event_type": "post_compress",
  "suggestion_id": "sugg-20260715-001",
  "suggestion_adopted": {
    "retained": ["turn-001", "turn-003"],
    "pruned": ["turn-004"],
    "injected": ["hook_id:175e30ff"]
  },
  "archived_turns": [
    { "user_message": {"text": "..."}, "llm_message": {"text": "..."} }
  ]
}
```

**用途**：
- 归档被压缩的轮次（复用现有 archive 链路）
- 记录建议采纳率（用于后续优化保留策略）
- 触发搜索索引更新

### 6.3 通信渠道

Cooperative 协议通过两个渠道暴露：

| 渠道 | 协议 | 适用场景 | 实现 |
|------|------|----------|------|
| MCP 工具 | stdio / Streamable HTTP | Agent 通过 MCP 协议接入（Claude Code / Cursor / Trae / OpenCode） | 新增 `pre_compress_hint` / `post_compress_ack` MCP 工具 |
| REST API | HTTP POST | Agent 通过 HTTP 接入（自定义 Agent / sidecar） | 新增 `POST /api/v1/cooperative/pre_compress` / `POST /api/v1/cooperative/post_compress` |

**设计原则**：两个渠道复用同一套核心逻辑（archive-core 层），仅入口层不同。

## 7. 状态机设计

### 7.1 协作状态机

```
                    ┌─────────┐
        ┌──────────→│  Idle    │←─────────────┐
        │           └────┬─────┘              │
        │                │                   │
        │                │ pre_compress_hint  │
        │                │ (Agent 通知)       │
        │                ↓                   │
        │           ┌─────────────┐          │
        │           │  Notified   │          │
        │           └──────┬──────┘          │
        │                  │                 │
        │                  │ 语义检索开始     │
        │                  ↓                 │
        │           ┌─────────────┐          │
        │           │  Analyzing  │          │
        │           └──────┬──────┘          │
        │                  │                 │
        │                  │ 生成保留建议     │
        │                  ↓                 │
        │           ┌─────────────┐          │
        │           │ Suggesting  │          │
        │           └──────┬──────┘          │
        │                  │                 │
        │                  │ 返回建议给 Agent │
        │                  ↓                 │
        │           ┌─────────────┐          │
        │           │  Awaiting   │          │
        │           │ (等待压缩)  │          │
        │           └──────┬──────┘          │
        │                  │                 │
        │                  │ post_compress   │
        │                  │ _ack (确认)     │
        │                  ↓                 │
        │           ┌─────────────┐          │
        │           │ Compressing │          │
        │           │ + Archive   │          │
        │           └──────┬──────┘          │
        │                  │                 │
        │                  │ 归档完成        │
        │                  └─────────────────┘
        │
        │  超时 / 错误
        │  (降级为 Independent)
        └──────────────────────────────────────┘
```

### 7.2 状态定义

| 状态 | 描述 | 入口条件 | 出口条件 | 超时 |
|------|------|----------|----------|------|
| **Idle** | 空闲，等待 Agent 请求 | 初始状态 / 上轮完成 | 收到 pre_compress_hint | 无 |
| **Notified** | 已收到压缩通知 | pre_compress_hint 到达 | 开始语义检索 | 5s |
| **Analyzing** | 正在语义检索 + 分析 | 检索开始 | 生成保留建议 | 10s |
| **Suggesting** | 已生成建议，准备返回 | 分析完成 | 返回 RetentionSuggestion | 1s |
| **Awaiting** | 等待 Agent 执行压缩 | 建议已返回 | 收到 post_compress_ack | 120s |
| **Compressing** | 归档被压缩内容 | post_compress_ack 到达 | 归档完成 | 30s |

### 7.3 超时与降级

**超时处理**：任一状态超时后，Cooperative 会话降级为 Independent 模式：

```
超时 → 记录 warn 日志 → 降级 Independent → Agent 独立执行压缩 → archive 归档
```

**降级保证**：Cooperative 的任何失败都不影响 Agent 的压缩操作，Agent 始终可以独立完成压缩并归档。

## 8. 接口契约（设计草案）

> 以下为 trait / struct 的设计草案，仅用于固化接口契约，不在本次实现。

### 8.1 CooperativeHandler trait

```rust
// crates/memory-center-archive-core/src/cooperative.rs（新增文件，设计草案）

use std::sync::Arc;
use crate::ArchiveResult;

/// Cooperative 协作处理器
///
/// 由 MemoryCenter 实现，Agent 通过 MCP/HTTP 调用。
/// Independent 模式下此 trait 不被调用。
#[async_trait::async_trait]
pub trait CooperativeHandler: Send + Sync {
    /// 压缩前通知 + 获取保留建议
    ///
    /// Agent 在执行压缩前调用，MemoryCenter 返回保留建议。
    /// 超时或失败时，Agent 应降级为 Independent 模式独立压缩。
    async fn pre_compress_hint(
        &self,
        request: PreCompressHintRequest,
    ) -> Result<RetentionSuggestion, CooperativeError>;

    /// 压缩后确认 + 归档
    ///
    /// Agent 在执行压缩后调用，归档被压缩内容并记录建议采纳率。
    /// 复用现有 archive 链路。
    async fn post_compress_ack(
        &self,
        request: PostCompressAckRequest,
    ) -> Result<ArchiveResult, CooperativeError>;

    /// 查询协作状态
    ///
    /// 返回当前 CooperativeSession 的状态（用于调试/监控）。
    async fn get_session_state(
        &self,
        session_id: &str,
    ) -> Result<CooperativeSessionState, CooperativeError>;
}
```

### 8.2 请求/响应结构

```rust
/// 压缩前通知请求
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PreCompressHintRequest {
    pub session_id: String,
    pub current_tokens: usize,
    pub token_threshold: usize,
    pub compression_scheme: String,
    pub context_snapshot: ContextSnapshot,
    pub turns_to_compress: Vec<TurnPreview>,
}

/// 上下文快照
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ContextSnapshot {
    pub current_task: Option<String>,
    pub recent_turns_summary: Option<String>,
    pub key_files: Vec<String>,
    pub tool_calls_summary: Vec<String>,
}

/// 轮次预览
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TurnPreview {
    pub turn_id: String,
    pub text_preview: String,
    pub token_count: usize,
}

/// 保留建议
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetentionSuggestion {
    pub session_id: String,
    pub suggestion_id: String,
    pub retain_turns: Vec<RetainItem>,
    pub prune_hints: Vec<PruneHint>,
    pub inject_memories: Vec<InjectItem>,
}

/// 保留项
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetainItem {
    pub turn_id: String,
    pub priority: Priority,
    pub reason: String,
    pub related_memories: Vec<String>,
}

/// 优先级
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub enum Priority {
    High,
    Medium,
    Low,
}

/// 修剪建议
#[derive(Debug, Clone, serde::Serialize)]
pub struct PruneHint {
    pub target: String,
    pub reason: String,
}

/// 注入项
#[derive(Debug, Clone, serde::Serialize)]
pub struct InjectItem {
    pub hook_id: String,
    pub reason: String,
    pub inject_strategy: InjectStrategy,
}

/// 注入策略
#[derive(Debug, Clone, serde::Serialize)]
pub enum InjectStrategy {
    /// 注入摘要
    Summary,
    /// 注入完整内容
    Full,
    /// 注入关键词
    Keywords,
}

/// 压缩后确认请求
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PostCompressAckRequest {
    pub session_id: String,
    pub suggestion_id: String,
    pub suggestion_adopted: SuggestionAdoption,
    pub archived_turns: Vec<crate::MessageTurn>,
}

/// 建议采纳记录
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SuggestionAdoption {
    pub retained: Vec<String>,
    pub pruned: Vec<String>,
    pub injected: Vec<String>,
}
```

### 8.3 错误类型

```rust
#[derive(Debug, thiserror::Error)]
pub enum CooperativeError {
    #[error("会话不存在: {0}")]
    SessionNotFound(String),

    #[error("状态非法: 当前 {current}, 期望 {expected}")]
    InvalidState { current: String, expected: String },

    #[error("语义检索失败: {0}")]
    SearchFailed(String),

    #[error("归档失败: {0}")]
    ArchiveFailed(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("降级: {reason}, 已切换为 Independent 模式")]
    Degraded { reason: String },
}
```

### 8.4 与 ArchiveEngine 的集成

```rust
// crates/memory-center-archive-core/src/lib.rs（扩展现有 ArchiveEngine）

pub struct ArchiveEngine {
    // ... 现有字段 ...

    /// Cooperative 处理器（v2.53 P8 新增，可选注入）
    ///
    /// 未注入时（Independent 模式），pre_compress 走现有逻辑。
    /// 注入后（Cooperative 模式），pre_compress 先调用 handler 获取建议。
    cooperative_handler: Option<Arc<dyn CooperativeHandler>>,
}

impl ArchiveEngine {
    /// 注入 Cooperative 处理器
    pub fn with_cooperative_handler(
        mut self,
        handler: Arc<dyn CooperativeHandler>,
    ) -> Self {
        self.cooperative_handler = Some(handler);
        self
    }
}
```

### 8.5 与 windows crate 的集成

```rust
// crates/memory-center-windows/src/cooperation.rs（扩展）

impl CooperationMode {
    pub fn is_supported(&self) -> bool {
        match self {
            Self::Independent => true,
            // P8 实现后改为 true
            Self::Cooperative => false,  // ← 实现后改为 true
        }
    }
}
```

## 9. 保留建议策略（语义检索）

### 9.1 检索流程

```
                    ┌───────────────────┐
                    │ context_snapshot   │
                    │ (Agent 提供)       │
                    └────────┬──────────┘
                             │
                             ↓
                    ┌───────────────────┐
                    │ 构建 query        │
                    │ (current_task +   │
                    │  recent_summary +  │
                    │  key_files)        │
                    └────────┬──────────┘
                             │
                    ┌────────┴──────────┐
                    │                   │
                    ↓                   ↓
          ┌─────────────┐     ┌─────────────┐
          │ BM25 检索   │     │ 向量检索    │
          │ (关键词)    │     │ (语义)      │
          └──────┬──────┘     └──────┬──────┘
                 │                   │
                 └────────┬──────────┘
                          │
                          ↓
                 ┌─────────────────┐
                 │ 融合排序         │
                 │ (RRF / 加权)     │
                 └────────┬────────┘
                          │
                          ↓
                 ┌─────────────────┐
                 │ 生成建议         │
                 │ (retain +       │
                 │  prune + inject) │
                 └─────────────────┘
```

### 9.2 query 构建

从 `context_snapshot` 构建检索 query：

```rust
fn build_search_query(snapshot: &ContextSnapshot) -> String {
    let mut parts = Vec::new();

    if let Some(task) = &snapshot.current_task {
        parts.push(task.clone());
    }
    if let Some(summary) = &snapshot.recent_turns_summary {
        parts.push(summary.clone());
    }
    for file in &snapshot.key_files {
        parts.push(file.clone());
    }

    parts.join(" ")
}
```

### 9.3 优先级判定规则

| 优先级 | 判定条件 | 示例 |
|--------|----------|------|
| **High** | turn 包含当前任务的架构决策 / 关键代码 / 用户确认的决策 | 包含"用户确认采用同步请求-响应"的轮次 |
| **Medium** | turn 包含相关技术参考 / 外部资料 / 后续可能引用的内容 | 包含 Anthropic context engineering 文章的轮次 |
| **Low** | turn 包含探索性搜索 / 工具原始输出 / 中间状态 | WebSearch 原始结果、Read 文件的完整内容 |

### 9.4 inject_memories 策略

当检索到的已归档记忆与当前上下文高度相关时，建议注入：

| 注入策略 | 适用场景 | 注入内容 |
|----------|----------|----------|
| **Summary** | 记忆是已完成任务的总结（如 P9 完成记录） | 记忆的摘要文本 |
| **Full** | 记忆是关键架构决策 / 用户规则 / 项目约束 | 记忆的完整内容 |
| **Keywords** | 记忆是技术参考 / 外部资料 | 关键词列表 + hook_id |

### 9.5 降级策略

当 Embedder API 不可用时（降级模式），保留建议退化为 BM25 关键词检索：

```
Embedder 可用 → BM25 + 向量检索 → 融合排序
Embedder 不可用 → 仅 BM25 关键词检索
两者都不可用 → 返回空建议（Agent 独立压缩）
```

## 10. 降级与容错

### 10.1 降级链

```
┌─────────────────────────────────────────────────────────────┐
│  Cooperative 模式启用？                                       │
│  ├─ 否 → Independent（现有行为，无变化）                      │
│  └─ 是 ↓                                                     │
│                                                             │
│  pre_compress_hint 调用成功？                                │
│  ├─ 否（网络错误 / 超时）→ Independent 降级                   │
│  │   (Agent 独立压缩 + archive 归档，warn 日志)              │
│  └─ 是 ↓                                                     │
│                                                             │
│  语义检索成功？                                              │
│  ├─ 否（Embedder 不可用）→ BM25 降级                         │
│  └─ 是 ↓                                                     │
│                                                             │
│  保留建议生成成功？                                          │
│  ├─ 否 → 返回空建议（Agent 独立决策压缩）                    │
│  └─ 是 ↓                                                     │
│                                                             │
│  ✅ 完整 Cooperative 流程                                   │
│  (Agent 采纳建议 + 压缩 + post_compress_ack 归档)           │
└─────────────────────────────────────────────────────────────┘
```

### 10.2 容错原则

| 原则 | 描述 |
|------|------|
| **不阻塞 Agent** | Cooperative 的任何失败都不阻塞 Agent 的压缩操作 |
| **不丢失数据** | 降级时仍通过 archive 归档被压缩内容 |
| **记录采纳率** | post_compress_ack 记录建议采纳率，用于优化策略 |
| **幂等性** | pre_compress_hint 可重试，不产生副作用 |

## 11. 测试策略

### 11.1 单元测试

| 测试范围 | 测试重点 | 文件 |
|----------|----------|------|
| 状态机 | 状态转换正确性、超时降级 | `cooperative.rs` |
| query 构建 | 从 ContextSnapshot 构建检索 query | `retention.rs` |
| 优先级判定 | High/Medium/Low 判定规则 | `retention.rs` |
| 降级链 | Embedder 不可用 → BM25 降级 | `retention.rs` |
| 错误处理 | 超时、网络错误、状态非法 | `cooperative.rs` |

### 11.2 集成测试

| 测试场景 | 验证点 |
|----------|--------|
| 完整 Cooperative 流程 | pre_compress → 建议返回 → post_compress → 归档完成 |
| Independent 降级 | Cooperative 失败 → Independent 降级 → archive 正常 |
| 语义检索准确性 | 已归档记忆被正确检索 → 建议保留相关轮次 |
| 建议采纳率记录 | post_compress_ack 正确记录 retained/pruned/injected |

### 11.3 端到端测试

| 测试场景 | 模拟方式 |
|----------|----------|
| Trae + MCP Cooperative | mock MCP 工具调用 pre_compress_hint |
| HTTP API Cooperative | curl 调用 /api/v1/cooperative/pre_compress |
| 长对话压缩 | 模拟 200K token 对话，验证保留建议效果 |

## 12. 风险评估与缓解

| 风险 | 等级 | 影响 | 缓解措施 |
|------|------|------|----------|
| **通信延迟** | 中 | pre_compress_hint 增加压缩前的等待时间 | 设置 10s 超时，超时降级 Independent |
| **建议质量低** | 中 | 语义检索不准导致建议无价值 | 记录采纳率，持续优化检索策略 |
| **状态不一致** | 低 | Agent 未发送 post_compress_ack 导致状态卡在 Awaiting | 120s 超时自动归位 Idle |
| **向后兼容** | 低 | Cooperative 可能影响现有 Independent 用户 | Cooperative 是可选注入，默认不启用 |
| **安全风险** | 低 | context_snapshot 可能包含敏感信息 | 支持脱敏配置，建议仅传摘要 |
| **性能压力** | 低 | 高频压缩导致语义检索压力 | 保留建议可缓存（semantic caching） |

## 13. 实现路线图

### 13.1 分阶段实现

| 阶段 | 内容 | 工作量 | 前置条件 | 状态 |
|------|------|--------|----------|------|
| **Phase 1** | trait 定义 + 数据结构 + 状态机 | 2h | 本设计文档评审通过 | ✅ 完成（23 单测） |
| **Phase 2** | archive-core 集成 + 语义检索逻辑 | 3h | Phase 1 | ✅ 完成（24 单测） |
| **Phase 3** | MCP 工具暴露 + HTTP API 端点 | 2h | Phase 2 | ✅ 完成（pre_compress_hint + post_compress_ack MCP 工具 + /api/v1/cooperative/* HTTP 端点） |
| **Phase 4** | 单测 + 集成测试 | 2h | Phase 3 | ✅ 完成（5 个 cooperative 集成测试，workspace 221+ 测试 0 失败） |
| **Phase 5** | windows crate `is_supported()` 改为 true + 联动测试 | 1h | Phase 4 | ✅ 完成（CooperationMode::Cooperative.is_supported() 返回 true） |
| **Phase 6** | 文档同步 + project_memory 更新 | 1h | Phase 5 | ✅ 完成（本文档 + preset-crates-inventory.md + project_memory.md 同步） |

**总工作量**：约 11h（含测试和文档），与原估计 8h+ 一致。

### 13.2 验证里程碑

| 里程碑 | 验证方式 | 状态 |
|--------|----------|------|
| Phase 1 完成 | `cargo build -p memory-center-archive-core` 通过 | ✅ |
| Phase 2 完成 | 语义检索单测通过 | ✅ |
| Phase 3 完成 | MCP 工具列表包含 pre_compress_hint | ✅ |
| Phase 4 完成 | 全量测试通过 | ✅ |
| Phase 5 完成 | `CooperationMode::Cooperative.is_supported()` 返回 true | ✅ |
| Phase 6 完成 | 文档同步 + project_memory 更新 | ✅ |

## 14. 附录

### 14.1 完整序列图

```
Agent                MemoryCenter           Embedder/Retriever
  │                       │                       │
  │  pre_compress_hint    │                       │
  │  (snapshot + turns)   │                       │
  │ ────────────────────→ │                       │
  │                       │                       │
  │                       │  semantic_search      │
  │                       │  (query from snapshot)│
  │                       │ ────────────────────→ │
  │                       │                       │
  │                       │  ←── results ──────  │
  │                       │                       │
  │                       │  生成建议             │
  │                       │  (retain+prune+inject)│
  │                       │                       │
  │  ←── suggestion ──── │                       │
  │  (RetentionSuggestion) │                       │
  │                       │                       │
  │  Agent 执行压缩        │                       │
  │  (采纳/忽略建议)       │                       │
  │                       │                       │
  │  post_compress_ack    │                       │
  │  (adopted + turns)    │                       │
  │ ────────────────────→ │                       │
  │                       │                       │
  │                       │  archive(turns)       │
  │                       │  (复用现有链路)        │
  │                       │                       │
  │  ←── archive_result ─ │                       │
  │                       │                       │
```

### 14.2 与现有组件的关系

```
┌─────────────────────────────────────────────────────────────┐
│                    Agent 工具                                │
│  (Claude Code / Cursor / Trae / OpenCode)                   │
└──────────┬──────────────────────────────────┬───────────────┘
           │                                  │
           │ MCP (stdio/HTTP)                 │ HTTP REST
           │                                  │
           ↓                                  ↓
┌──────────────────┐              ┌──────────────────┐
│  memory-center   │              │  memory-center    │
│  -mcp            │              │  -server          │
│                  │              │                   │
│  新增工具：       │              │  新增端点：        │
│  pre_compress    │              │  /api/v1/         │
│  _hint           │              │  cooperative/     │
│  post_compress   │              │  pre_compress     │
│  _ack            │              │  post_compress    │
└────────┬─────────┘              └────────┬──────────┘
         │                                  │
         │          共享核心逻辑             │
         └──────────┬───────────────────────┘
                    ↓
         ┌──────────────────────┐
         │  memory-center-       │
         │  archive-core         │
         │                       │
         │  ArchiveEngine        │
         │  + CooperativeHandler │  ← P8 新增
         │    trait              │
         │  + RetentionBuilder  │  ← P8 新增
         │    (语义检索 + 建议)   │
         └──────────┬───────────┘
                    │
                    ↓
         ┌──────────────────────┐
         │  memory-center-       │
         │  search               │
         │  (BM25 + 向量检索)    │
         └──────────────────────┘
```

### 14.3 与 P7 MemoryLink 的协同

P7 Phase 3 的 `write_standalone_memory` / `write_linked_memory` 与 Cooperative 的 `inject_memories` 形成闭环：

```
Cooperative 的 inject_memories 建议注入已归档记忆
    ↓
Agent 看到建议后，可主动调用 write_standalone_memory
将关键信息写入 standalone 记忆（持久化）
    ↓
下次 Cooperative 时，standalone 记忆可被 semantic_search 检索到
    ↓
形成"归档 → 检索 → 建议注入 → 主动持久化 → 再次检索"的增强循环
```

### 14.4 配置项设计

```toml
# Cooperative 模式配置（config.toml 或环境变量）

[cooperative]
# 是否启用 Cooperative 模式（false 时走 Independent）
enabled = false

# pre_compress_hint 超时（秒）
timeout_hint = 10

# Awaiting 状态超时（秒，等待 Agent post_compress_ack）
timeout_awaiting = 120

# 语义检索 top_k
search_top_k = 5

# 保留建议最大项数
max_retain_items = 10

# context_snapshot 脱敏级别
# full: 完整内容 / summary: 仅摘要 / keywords: 仅关键词
snapshot_sanitize_level = "summary"

# 是否缓存保留建议（semantic caching）
cache_enabled = true
```

### 14.5 与 Independent 模式的对比

| 维度 | Independent | Cooperative |
|------|-------------|-------------|
| 通信方向 | 单向（Agent → MC） | 双向（Agent ↔ MC） |
| 归档时机 | 压缩后 | 压缩前 + 压缩后 |
| 保留决策 | Agent 独立决定 | MC 提供建议，Agent 最终决定 |
| 记忆利用 | 被动归档 | 主动检索 + 建议注入 |
| 复杂度 | 低 | 中（状态机 + 语义检索） |
| 延迟 | 无额外延迟 | +10-15s（语义检索） |
| 适用场景 | 短对话 / 简单任务 | 长对话 / 复杂任务 / 多阶段开发 |

## 15. 参考文档

- [Anthropic: Effective Context Engineering for AI Agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) — Compaction / Note-taking / Sub-agent 三策略
- [Redis: Context Window Overflow](https://redis.io/blog/context-window-overflow/) — 五种 overflow 处理策略
- [MemGPT: Teaching LLMs memory management](https://github.com/cpacker/MemGPT) — 三层内存架构
- [preset-crates-inventory.md](preset-crates-inventory.md) 章节 4 — windows crate 完整配置参考
- [preset-crates-architecture.md](preset-crates-architecture.md) — P8 路线图与架构定位
- [sentencepiece-guide.md](sentencepiece-guide.md) — Feature Gating 模式参考（Cooperative 可借鉴）
