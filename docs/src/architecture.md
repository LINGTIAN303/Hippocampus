# 整体架构

> 本章节是 [GitHub Wiki: Architecture](https://github.com/LINGTIAN303/MemoryCenter/wiki/Architecture) 的镜像。

## 三层分层架构

```
Layer 3: Bindings       ① Python 原生绑定 (PyO3)  ② WASM 组件  ③ Node/Go/Java
Layer 2: Interface      ① C ABI  ② Axum HTTP REST  ③ MCP stdio  ④ MCP Streamable HTTP
Layer 1: Core (Rust)    纯逻辑 crate（core-logic）+ facade crate（core）
```

## 架构图

```mermaid
flowchart TB
    subgraph L3["Layer 3 · Bindings"]
        PY["Python (PyO3)"]
        WASM["WASM"]
        NODE["Node.js (napi-rs)"]
    end
    subgraph L2["Layer 2 · Interface"]
        FFI["C ABI"]
        HTTP["HTTP REST (Axum)"]
        MCP["MCP Server"]
    end
    subgraph L1["Layer 1 · Core (Rust)"]
        CORE["memory-center-core (facade)"]
        LOGIC["core-logic (纯逻辑)"]
        SEARCH["search (BM25 + 语义)"]
        LLM["llm (摘要+冲突)"]
        PRESET["presets (11 Agent + 7 Scenario)"]
    end
    subgraph STORE["Storage"]
        SQLITE["SQLite (含向量)"]
        FILE["文件树"]
    end

    PY --> FFI
    WASM --> LOGIC
    NODE --> FFI
    FFI --> CORE
    HTTP --> CORE
    MCP --> CORE
    CORE --> LOGIC
    CORE --> SEARCH
    CORE --> LLM
    CORE --> PRESET
    LOGIC --> STORE
    SEARCH --> STORE
```

## 设计原则

1. **纯逻辑与 IO 分离**：`core-logic` 无 IO 依赖，可编译为 WASM；`core` 作为 facade 整合原生实现
2. **可插拔架构**：`Storage` / `Scorer` / `Migrator` 等 trait 均可替换实现
3. **接入层无状态**：HTTP / MCP server 无状态，水平扩展友好
4. **跨语言一致性**：所有接入方式共享同一组核心操作（archive / retrieve / summaries / prompt / compaction）

## 数据流

### 归档流程

```mermaid
sequenceDiagram
    autonumber
    participant Agent
    participant MCP as MCP Server
    participant Core as Core
    participant Store as Storage

    Agent->>MCP: archive(session_id, turns_json)
    MCP->>Core: archive(turns)
    Core->>Store: freeze_context()
    Core->>Store: 生成索引钩子
    Store-->>Core: hook_id
    Core-->>MCP: 归档摘要
    MCP-->>Agent: 归档成功
```

### 检索流程

```mermaid
sequenceDiagram
    autonumber
    participant Agent
    participant MCP as MCP Server
    participant Core as Core
    participant Store as Storage

    Agent->>MCP: prompt(session_id)
    MCP->>Core: render_prompt()
    Core->>Store: load_summaries()
    Store-->>Core: 摘要钩子列表
    Core-->>MCP: prompt 文本
    MCP-->>Agent: 注入 system prompt

    Note over Agent,Store: LLM 需要细节时
    Agent->>MCP: retrieve(hook_id)
    MCP->>Core: retrieve(hook_id)
    Core->>Store: load_full_turns()
    Store-->>Agent: 完整对话上下文
```

## 详细文档

完整的架构设计文档见 [docs/ARCHITECTURE.md](https://github.com/LINGTIAN303/MemoryCenter/blob/main/docs/ARCHITECTURE.md)（仓库内）与 [Wiki: Architecture](https://github.com/LINGTIAN303/MemoryCenter/wiki/Architecture)。
