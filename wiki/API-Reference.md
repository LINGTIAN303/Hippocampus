# API Reference

MemoryCenter HTTP REST API 完整参考文档。涵盖所有端点的请求参数、响应结构、示例与错误码。

## 1. 概览

MemoryCenter HTTP REST API 基于 Axum 0.8 实现，将核心库的归档 / 检索 / 周期管理 / 冲突检测等能力暴露为标准 REST 接口，供任何语言（Python / Node / Go / Java / curl 等）通过 HTTP 调用。

| 项目 | 说明 |
|------|------|
| API 前缀 | `/api/v1` |
| 默认监听地址 | `127.0.0.1:8765` |
| 传输协议 | HTTP/1.1（JSON 请求 / JSON 响应） |
| 状态模型 | 无状态设计，每次请求独立创建 Storage，支持水平扩展 |
| 鉴权 | 可选 API Key（`MEMORY_CENTER_API_KEY` 环境变量驱动） |
| MCP 端点 | `/mcp`（v2.36+，与 REST API 共享 Axum 服务，独立鉴权） |
| 当前版本 | v2.37 |

## 2. 服务启动

### 2.1 核心环境变量

| 环境变量 | 说明 | 默认值 |
|---------|------|--------|
| `MEMORY_CENTER_ROOT` | 存储根目录（记忆文件 / 索引 / raw_context 的根路径） | `./data` |
| `MEMORY_CENTER_HOST` | 监听地址 | `127.0.0.1` |
| `MEMORY_CENTER_PORT` | 监听端口 | `8765` |
| `MEMORY_CENTER_API_KEY` | API Key（配置后所有请求需携带 `Authorization: Bearer <key>`） | 空（不鉴权） |

### 2.2 MCP Streamable HTTP 环境变量（v2.36+）

| 环境变量 | 说明 | 默认值 |
|---------|------|--------|
| `MEMORY_CENTER_MCP_ENABLED` | 启用 `/mcp` 端点 | `false`（需显式启用） |
| `MEMORY_CENTER_MCP_STATEFUL` | 启用 session 模式 | `true` |
| `MEMORY_CENTER_MCP_ALLOWED_HOSTS` | 允许的 Host 列表（逗号分隔） | `localhost,127.0.0.1,::1` |
| `MEMORY_CENTER_MCP_ALLOWED_ORIGINS` | 允许的 Origin 列表（逗号分隔） | 空（不校验 Origin） |

### 2.3 可选 LLM 组件环境变量

以下组件未配置时自动降级为启发式实现，归档主流程不中断。

| 组件 | 环境变量前缀 | 说明 | 降级行为 |
|------|-------------|------|---------|
| 摘要生成器 | `MEMORY_CENTER_GENERATOR_*` | LLM 生成结构化摘要（title/abstract/key_facts） | 启发式（首条消息前 80 字符） |
| 冲突检测器 | `MEMORY_CENTER_DETECTOR_*` | LLM 三维度冲突检测 | HeuristicDetector（纯算法） |
| 语义检索 | `MEMORY_CENTER_EMBEDDER_*` | Embedding 向量检索 | 仅 BM25 关键词检索 |

每组环境变量包含：`_API_URL` / `_API_KEY` / `_MODEL` / `_TIMEOUT`，摘要生成器和冲突检测器额外含 `_MAX_TOKENS`，语义检索含 `_DIM`。详见 [Deployment](Deployment)。

### 2.4 启动命令

```bash
# 开发模式
MEMORY_CENTER_ROOT=./data MEMORY_CENTER_HOST=0.0.0.0 MEMORY_CENTER_PORT=8765 \
  cargo run -p memory-center-server

# 启用 MCP Streamable HTTP（v2.36+）
MEMORY_CENTER_MCP_ENABLED=true MEMORY_CENTER_ROOT=./data \
  cargo run -p memory-center-server

# 生产模式（编译好的二进制）
./memory-center-server
```

启动后日志会列出所有可用端点。

## 3. 端点一览表

共 17 个 REST 端点，按功能分组如下。

### 3.1 归档与检索

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/sessions/{sid}/archive` | 归档一批轮次为记忆文件 |
| POST | `/api/v1/sessions/{sid}/pre-compress` | 压缩前一次性完整归档（v2.34） |
| GET | `/api/v1/sessions/{sid}/memories/{hook_id}` | 按 hook_id 检索完整记忆文件 |
| GET | `/api/v1/sessions/{sid}/summaries` | 获取所有周期摘要列表 |
| GET | `/api/v1/sessions/{sid}/prompt` | 渲染摘要为 system prompt 文本 |
| POST | `/api/v1/sessions/{sid}/compaction` | 触发周期任务（周级合并 / 月级淘汰） |

### 3.2 记忆更新

| 方法 | 路径 | 说明 |
|------|------|------|
| PATCH | `/api/v1/sessions/{sid}/memories/{hook_id}` | 更新记忆（added/revised/deprecated facts） |

### 3.3 冲突检测

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/sessions/{sid}/memories/{hook_id}/conflicts` | 查询指定记忆的冲突历史记录 |
| POST | `/api/v1/sessions/{sid}/memories/{hook_id}/detect-conflicts` | 冲突预检测（不实际写入） |

### 3.4 批量操作

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/sessions/{sid}/memories/batch-retrieve` | 批量检索记忆文件 |
| POST | `/api/v1/sessions/{sid}/memories/batch-delete` | 批量删除记忆文件（软删除） |
| POST | `/api/v1/sessions/{sid}/memories/batch-update` | 批量更新记忆文件 |

### 3.5 语义检索

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/sessions/{sid}/search` | 语义检索记忆（BM25 + 向量混合） |

### 3.6 预设管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/presets/agents` | 列出 11 个内置 Agent |
| GET | `/api/v1/presets/scenarios` | 列出 7 个内置 Scenario |
| GET | `/api/v1/presets/models` | 列出所有 ModelVariant |
| POST | `/api/v1/presets/build` | 即时构建预设配置 |

### 3.7 MCP 端点（v2.36+）

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/mcp` | MCP 请求（JSON-RPC 2.0） |
| GET | `/mcp` | MCP SSE 流 |
| DELETE | `/mcp` | 关闭 MCP session |

`/mcp` 不经过 REST API 的 API Key 鉴权，使用 MCP 协议自身认证。详见第 8 节。

## 4. 端点详情

### 4.1 POST /api/v1/sessions/{sid}/archive

归档一批轮次为记忆文件，生成索引钩子（IndexHook）。归档后自动触发搜索索引。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |

**请求体（ArchiveRequest）**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `turns` | `MessageTurn[]` | 是 | 待归档的轮次列表（不能为空） |
| `project_id` | string | 否 | 项目 ID（影响存储路径，多项目隔离） |
| `preset` | `PresetRequest` | 否 | 预设配置（v2.29，覆盖归档阈值 / 摘要模板） |

**PresetRequest 结构**

| 字段 | 类型 | 说明 |
|------|------|------|
| `agent` | string | Agent display_name（如 "Claude Code"） |
| `scenario` | string | Scenario 名称（大小写不敏感，如 "coding"） |
| `model` | string | ModelVariant 名称 |
| `archive_threshold` | number | 用户覆盖归档阈值 |
| `summary_template` | string | 用户覆盖摘要模板（需含 `{conversation}`） |

**curl 示例**

```bash
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/archive \
  -H "Content-Type: application/json" \
  -d '{
    "turns": [
      {
        "user_message": {"text": "你好", "attachments": [], "tool_calls": [], "thinking": null},
        "llm_message": {"text": "你好！有什么可以帮你？", "attachments": [], "tool_calls": [], "thinking": null},
        "tags": [{"kind": "Text"}],
        "token_count": 20
      }
    ],
    "project_id": "proj-a"
  }'
```

**响应（200，SummaryView）**

```json
{
  "hook_id": "550e8400-e29b-41d4-a716-446655440000",
  "memory_id": "mem-20260708-abc123",
  "summary_title": "你好",
  "tags": ["文本消息"],
  "archived_at": "2026-07-08T10:30:00Z",
  "period": "daily",
  "token_count": 20
}
```

> 说明：`abstract_text` / `key_facts` / `key_entities` / `clue_anchors` 字段在日级归档时为空（`skip_serializing_if`），周级/月级或配置 LLM 摘要生成器后才有值。

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 400 | `BAD_REQUEST` | `turns` 为空 / preset 构建失败 |
| 500 | `INTERNAL_ERROR` | 归档过程内部错误 |

---

### 4.2 POST /api/v1/sessions/{sid}/pre-compress

压缩前一次性完整归档（v2.34）。与 `archive` 互补：输入 `full_context` 完整字符串而非结构化 `turns`，双轨处理（raw_context 原样保存 + 尝试解析为 turns 复用 Archiver）。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |

**请求体（PreCompressRequest）**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `full_context` | string | 是 | 完整上下文字符串（支持 JSON 数组或 `User:`/`Assistant:` 分隔符格式） |
| `estimated_tokens` | number | 否 | 客户端估算的原始 token 数（未传时按 `len()/3` 估算） |
| `preset` | `PresetRequest` | 否 | 预设配置 |
| `task_state_snapshot` | `TaskStateSnapshotRequest` | 否 | 任务状态快照（压缩后校准用） |
| `project_id` | string | 否 | 项目 ID |

**TaskStateSnapshotRequest 结构**

| 字段 | 类型 | 说明 |
|------|------|------|
| `current_task` | string | 当前任务名称 |
| `completed_steps` | string[] | 已完成步骤列表 |
| `in_progress_step` | string | 进行中步骤（被压缩打断的任务） |
| `next_step` | string | 下一建议步骤 |

**curl 示例**

```bash
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/pre-compress \
  -H "Content-Type: application/json" \
  -d '{
    "full_context": "User: 你好\nAssistant: 你好！有什么可以帮你？",
    "estimated_tokens": 1500,
    "task_state_snapshot": {
      "current_task": "批次A-数据完整性修复",
      "completed_steps": ["步骤1", "步骤2"],
      "in_progress_step": "步骤3",
      "next_step": "步骤4"
    }
  }'
```

**响应（200，JSON）**

```json
{
  "hook_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "raw_context_path": "data/sessions/sess-001/raw/a1b2c3d4.txt",
  "parse_success": true,
  "parsed_turns_count": 1,
  "archived_tokens": 1500,
  "estimated_total_tokens": 1500,
  "threshold": 120000,
  "threshold_ratio_percent": 1,
  "suggestion": "压缩前归档完成，共 1 轮，原始 ~1500 tokens（阈值 120000，当前 1%）。可安全压缩。",
  "archived_at": "2026-07-08T10:35:00Z"
}
```

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 400 | `BAD_REQUEST` | `full_context` 为空 |
| 500 | `INTERNAL_ERROR` | 写 raw_context 失败（核心兜底，阻塞返回） |

---

### 4.3 GET /api/v1/sessions/{sid}/memories/{hook_id}

按钩子 ID 检索完整记忆文件（含所有轮次的完整内容，非摘要）。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |
| `hook_id` | string | 钩子 ID（archive 返回的 `hook_id`） |

**查询参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `project_id` | string | 项目 ID（可选，多项目隔离时必传） |

**curl 示例**

```bash
curl "http://localhost:8765/api/v1/sessions/sess-001/memories/550e8400-e29b-41d4-a716-446655440000?project_id=proj-a"
```

**响应（200，MemoryFile）**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "schema_version": 1,
  "archived_at": "2026-07-08T10:30:00Z",
  "session_id": "sess-001",
  "project_id": "proj-a",
  "turns": [
    {
      "id": "11111111-2222-3333-4444-555555555555",
      "user_message": {"text": "你好", "attachments": [], "tool_calls": [], "thinking": null},
      "llm_message": {"text": "你好！有什么可以帮你？", "attachments": [], "tool_calls": [], "thinking": null},
      "tags": [{"kind": "Text"}],
      "timestamp": "2026-07-08T10:29:50Z",
      "token_count": 20
    }
  ],
  "tags": [{"kind": "Text"}],
  "total_tokens": 20,
  "truncated": false,
  "period": "Daily",
  "access_count": 1,
  "importance": 0,
  "updates": []
}
```

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 404 | `NOT_FOUND` | hook_id 不存在 / 记忆文件已删除 |

---

### 4.4 GET /api/v1/sessions/{sid}/summaries

获取当前会话所有周期的摘要视图列表（摘要钩子，轻量，用于注入 system prompt）。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |

**查询参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `project_id` | string | 项目 ID（可选） |

**curl 示例**

```bash
curl "http://localhost:8765/api/v1/sessions/sess-001/summaries"
```

**响应（200，SummaryView[]）**

```json
[
  {
    "hook_id": "550e8400-e29b-41d4-a716-446655440000",
    "memory_id": "mem-20260708-abc123",
    "summary_title": "你好",
    "tags": ["文本消息"],
    "archived_at": "2026-07-08T10:30:00Z",
    "period": "daily",
    "token_count": 20
  },
  {
    "hook_id": "660f9500-f30c-52e5-b827-557766551111",
    "memory_id": "mem-20260701-def456",
    "summary_title": "周度合并：项目架构讨论",
    "abstract_text": "本周讨论了 MemoryCenter 的三层架构设计和 crate 拆分方案",
    "key_facts": ["采用 core-logic + core 双层设计", "新增 WASM 组件"],
    "key_entities": ["MemoryCenter", "Rust", "WASM"],
    "tags": ["文本消息", "代码块"],
    "archived_at": "2026-07-01T20:00:00Z",
    "period": "weekly",
    "token_count": 8500
  }
]
```

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 500 | `INTERNAL_ERROR` | 读取索引文件失败 |

---

### 4.5 GET /api/v1/sessions/{sid}/prompt

渲染所有周期摘要为 system prompt 文本，直接拼接给 LLM。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |

**查询参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `project_id` | string | 项目 ID（可选） |

**curl 示例**

```bash
curl "http://localhost:8765/api/v1/sessions/sess-001/prompt"
```

**响应（200，PromptResponse）**

```json
{
  "prompt": "# 可用记忆索引\n\n## daily\n- [你好] hook=550e8400... tokens=20 2026-07-08\n\n## weekly\n- [周度合并：项目架构讨论] hook=660f9500... tokens=8500 2026-07-01\n  摘要：本周讨论了 MemoryCenter 的三层架构设计和 crate 拆分方案\n  关键事实：采用 core-logic + core 双层设计 / 新增 WASM 组件\n"
}
```

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 500 | `INTERNAL_ERROR` | 渲染失败 |

---

### 4.6 POST /api/v1/sessions/{sid}/compaction

触发周期任务。周级执行无损去重合并，月级执行 4 维评分淘汰。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |

**请求体（CompactionRequest）**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `period` | string | 是 | 周期类型：`"weekly"` 或 `"monthly"` |
| `project_id` | string | 否 | 项目 ID |

**curl 示例**

```bash
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/compaction \
  -H "Content-Type: application/json" \
  -d '{"period": "weekly", "project_id": "proj-a"}'
```

**响应（200，CompactionResult）**

```json
{
  "memory_file_id": "mem-20260708-weekly-xyz789",
  "total_turns": 42,
  "total_tokens": 15600,
  "hooks_count": 7,
  "period": "weekly"
}
```

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 400 | `BAD_REQUEST` | `period` 值无效（仅支持 weekly / monthly） |
| 500 | `INTERNAL_ERROR` | 合并 / 淘汰过程失败 |

---

### 4.7 PATCH /api/v1/sessions/{sid}/memories/{hook_id}

按钩子 ID 更新记忆文件（added / revised / deprecated facts）。更新前同步检测冲突并持久化冲突记录。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |
| `hook_id` | string | 钩子 ID |

**请求体（UpdateMemoryRequest）**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `added_facts` | string[] | 否 | 新增的事实列表 |
| `revised_facts` | string[] | 否 | 修正的事实列表 |
| `deprecated_facts` | string[] | 否 | 废弃的事实列表 |
| `project_id` | string | 否 | 项目 ID |

> 至少需要一项 added / revised / deprecated facts，否则返回 400。

**curl 示例**

```bash
curl -X PATCH http://localhost:8765/api/v1/sessions/sess-001/memories/550e8400-e29b-41d4-a716-446655440000 \
  -H "Content-Type: application/json" \
  -d '{
    "added_facts": ["项目使用 Rust + Axum 技术栈"],
    "revised_facts": ["部署端口改为 8765"],
    "deprecated_facts": ["旧的 9000 端口配置"]
  }'
```

**响应（200，UpdateMemoryResponse）**

```json
{
  "success": true,
  "added": 1,
  "revised": 1,
  "deprecated": 1,
  "conflicts": 0,
  "has_critical": false
}
```

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 400 | `BAD_REQUEST` | 三类 facts 全为空 |
| 404 | `NOT_FOUND` | hook_id 不存在 |
| 500 | `INTERNAL_ERROR` | 更新持久化失败 |

---

### 4.8 GET /api/v1/sessions/{sid}/memories/{hook_id}/conflicts

获取指定记忆文件的所有冲突记录（来自历史 updates 的 conflicts 字段，按时间顺序扁平化）。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |
| `hook_id` | string | 钩子 ID |

**查询参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `project_id` | string | 项目 ID（可选） |

**curl 示例**

```bash
curl "http://localhost:8765/api/v1/sessions/sess-001/memories/550e8400/conflicts"
```

**响应（200，ConflictsResponse）**

```json
{
  "total": 2,
  "critical_count": 1,
  "conflicts": [
    {
      "kind": "DirectContradiction",
      "severity": "Critical",
      "new_fact": "项目使用 Python",
      "existing_fact": "项目使用 Rust",
      "description": "直接矛盾：新事实与已有事实冲突"
    },
    {
      "kind": "SelfContradiction",
      "severity": "Warning",
      "new_fact": "部署在 8080",
      "existing_fact": "部署在 8765",
      "description": "自我矛盾：同批次内事实不一致"
    }
  ]
}
```

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 404 | `NOT_FOUND` | hook_id 不存在 |
| 500 | `INTERNAL_ERROR` | 读取记忆文件失败 |

---

### 4.9 POST /api/v1/sessions/{sid}/memories/{hook_id}/detect-conflicts

冲突预检测（不实际写入）。读取 IndexHook 的 `summary.key_facts` 作为历史事实集，检测本次更新的潜在冲突。用于 Agent 在 update 前评估风险。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |
| `hook_id` | string | 钩子 ID |

**请求体（UpdateMemoryRequest）**

同 4.7 节的 `UpdateMemoryRequest`。

**curl 示例**

```bash
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/memories/550e8400/detect-conflicts \
  -H "Content-Type: application/json" \
  -d '{"added_facts": ["项目使用 Python"]}'
```

**响应（200，ConflictsResponse）**

同 4.8 节的 `ConflictsResponse`。

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 400 | `BAD_REQUEST` | 三类 facts 全为空 |
| 404 | `NOT_FOUND` | hook_id 不存在 |

---

### 4.10 POST /api/v1/sessions/{sid}/memories/batch-retrieve

批量按 hook_id 列表检索记忆文件。单个失败不影响其他条目。内部使用 Semaphore 限制 8 并发，结果顺序与输入一致。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |

**请求体（BatchRetrieveRequest）**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `hook_ids` | string[] | 是 | 要检索的 hook_id 列表（不能为空） |
| `project_id` | string | 否 | 项目 ID |

**curl 示例**

```bash
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/memories/batch-retrieve \
  -H "Content-Type: application/json" \
  -d '{
    "hook_ids": ["550e8400-...", "660f9500-..."],
    "project_id": "proj-a"
  }'
```

**响应（200，BatchRetrieveItem[]）**

```json
[
  {
    "hook_id": "550e8400-...",
    "success": true,
    "data": {"id": "550e8400-...", "turns": [...], "total_tokens": 20}
  },
  {
    "hook_id": "660f9500-...",
    "success": false,
    "error": "未找到钩子 ID: 660f9500-..."
  }
]
```

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 400 | `BAD_REQUEST` | `hook_ids` 为空 |

---

### 4.11 POST /api/v1/sessions/{sid}/memories/batch-delete

批量按 hook_id 列表删除记忆文件。采用软删除方案：删除记忆文件 + 索引钩子标记为 `Deleted` + 清理内存搜索索引。单个失败不影响其他条目。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |

**请求体（BatchDeleteRequest）**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `hook_ids` | string[] | 是 | 要删除的 hook_id 列表 |
| `project_id` | string | 否 | 项目 ID |

**curl 示例**

```bash
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/memories/batch-delete \
  -H "Content-Type: application/json" \
  -d '{"hook_ids": ["550e8400-...", "660f9500-..."]}'
```

**响应（200，BatchDeleteItem[]）**

```json
[
  {"hook_id": "550e8400-...", "success": true},
  {"hook_id": "660f9500-...", "success": false, "error": "未找到对应的 memory_id"}
]
```

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 400 | `BAD_REQUEST` | `hook_ids` 为空 |

---

### 4.12 POST /api/v1/sessions/{sid}/memories/batch-update

批量按 hook_id 列表更新记忆文件。单个失败不影响其他条目。配置冲突检测器时每条更新都会检测冲突。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |

**请求体（BatchUpdateRequest）**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `updates` | `BatchUpdateEntry[]` | 是 | 更新条目列表 |
| `project_id` | string | 否 | 项目 ID |

**BatchUpdateEntry 结构**

| 字段 | 类型 | 说明 |
|------|------|------|
| `hook_id` | string | 钩子 ID |
| `added_facts` | string[] | 新增事实 |
| `revised_facts` | string[] | 修正事实 |
| `deprecated_facts` | string[] | 废弃事实 |

**curl 示例**

```bash
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/memories/batch-update \
  -H "Content-Type: application/json" \
  -d '{
    "updates": [
      {"hook_id": "550e8400-...", "added_facts": ["fact A"]},
      {"hook_id": "660f9500-...", "revised_facts": ["fact B 修正"]}
    ]
  }'
```

**响应（200，BatchUpdateItem[]）**

```json
[
  {
    "hook_id": "550e8400-...",
    "success": true,
    "added": 1,
    "revised": 0,
    "deprecated": 0,
    "conflicts": 0,
    "has_critical": false
  },
  {
    "hook_id": "660f9500-...",
    "success": false,
    "error": "未找到钩子 ID: 660f9500-..."
  }
]
```

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 400 | `BAD_REQUEST` | `updates` 为空 |

---

### 4.13 POST /api/v1/sessions/{sid}/search

语义检索记忆文件（BM25 关键词 + Embedding 向量混合检索）。需配置 `MEMORY_CENTER_EMBEDDER_*` 环境变量，未配置时返回 501。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `sid` | string | 会话 ID |

**请求体（SearchRequest）**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `query` | string | 是 | 搜索查询文本（不能为空） |
| `top_k` | number | 否 | 返回 top-K 结果（默认 5） |

**curl 示例**

```bash
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/search \
  -H "Content-Type: application/json" \
  -d '{"query": "项目使用什么技术栈", "top_k": 3}'
```

**响应（200，SearchResponse）**

```json
{
  "results": [
    {
      "hook_id": "550e8400-...",
      "memory_id": "mem-20260708-abc123",
      "summary_title": "技术栈讨论",
      "score": 0.85,
      "source": "Hybrid",
      "snippet": "项目使用 Rust + Axum + SQLite 技术栈..."
    }
  ],
  "mode": "hybrid"
}
```

> `mode` 可选值：`keyword`（仅 BM25）/ `semantic`（仅向量）/ `hybrid`（混合）/ `empty`（无结果）。

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 400 | `BAD_REQUEST` | `query` 为空 |
| 501 | `NOT_IMPLEMENTED` | 未配置 Embedder API（语义检索不可用） |

---

### 4.14 GET /api/v1/presets/agents

列出所有内置 Agent（11 个）。

**curl 示例**

```bash
curl http://localhost:8765/api/v1/presets/agents
```

**响应（200，AgentInfo[]）**

```json
[
  {"name": "Claude Code", "session_prefix": "claude-code", "is_mainstream": true},
  {"name": "Cursor", "session_prefix": "cursor", "is_mainstream": true},
  {"name": "Trae", "session_prefix": "trae", "is_mainstream": true},
  {"name": "Codex CLI", "session_prefix": "codex", "is_mainstream": true}
]
```

> 完整列表含 11 个 Agent，以上为前 4 个主流 Agent 示例。

---

### 4.15 GET /api/v1/presets/scenarios

列出所有内置 Scenario（7 个）。

**curl 示例**

```bash
curl http://localhost:8765/api/v1/presets/scenarios
```

**响应（200，ScenarioInfo[]）**

```json
[
  {"variant": "Coding", "display_name": "编码场景", "archive_threshold": 400000},
  {"variant": "Writing", "display_name": "写作场景", "archive_threshold": 200000},
  {"variant": "Research", "display_name": "研究场景", "archive_threshold": 300000},
  {"variant": "Daily", "display_name": "日常场景", "archive_threshold": 100000},
  {"variant": "Finance", "display_name": "金融场景", "archive_threshold": 250000},
  {"variant": "Design", "display_name": "设计场景", "archive_threshold": 200000},
  {"variant": "OfficeWork", "display_name": "办公场景", "archive_threshold": 150000}
]
```

---

### 4.16 GET /api/v1/presets/models

列出所有 ModelVariant。

**curl 示例**

```bash
curl http://localhost:8765/api/v1/presets/models
```

**响应（200，ModelInfo[]）**

```json
[
  {
    "name": "claude-opus-4.8",
    "family": "Anthropic Claude",
    "context_window": 200000,
    "is_default": true
  },
  {
    "name": "claude-sonnet-5",
    "family": "Anthropic Claude",
    "context_window": 200000,
    "is_default": false
  }
]
```

> 完整列表含所有内置 ModelVariant，以上为示例。

---

### 4.17 POST /api/v1/presets/build

即时构建预设配置，返回最终生效值。用于预检预设效果后再调用 archive。

**请求体（BuildPresetRequest）**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `agent` | string | 否 | Agent display_name（如 "Claude Code"） |
| `scenario` | string | 否 | Scenario 名称（大小写不敏感） |
| `model` | string | 否 | ModelVariant 名称 |
| `archive_threshold` | number | 否 | 用户覆盖归档阈值（最高优先级） |
| `summary_template` | string | 否 | 用户覆盖摘要模板（需含 `{conversation}`） |

**curl 示例**

```bash
curl -X POST http://localhost:8765/api/v1/presets/build \
  -H "Content-Type: application/json" \
  -d '{"agent": "Claude Code", "scenario": "coding"}'
```

**响应（200，JSON）**

```json
{
  "archive_threshold": 400000,
  "summary_template": "请总结以下对话...",
  "session_prefix": "claude-code",
  "archive_to_MemoryCenter": true,
  "has_agent": true,
  "has_scenario": true,
  "has_window": true,
  "has_model": false,
  "skills_count": 5
}
```

**错误码**

| HTTP | code | 触发条件 |
|------|------|---------|
| 400 | `BAD_REQUEST` | model 未找到 / summary_template 缺少 `{conversation}` |

## 5. 请求/响应数据结构

### 5.1 MessageTurn

一轮对话的完整内容（非摘要）。

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `id` | UUID | 服务端自动生成 | 轮次唯一 ID |
| `user_message` | `MessageContent` | - | 用户消息内容 |
| `llm_message` | `MessageContent` | - | LLM 消息内容 |
| `tags` | `Tag[]` | `[{"kind":"Text"}]` | 该轮次的标签集合 |
| `timestamp` | RFC3339 | 服务端当前时间 | 时间戳 |
| `token_count` | number | 0 | 该轮次消耗的 token 数 |

### 5.2 MessageContent

消息内容（支持多种媒介）。

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `text` | string | null | 文本部分（可能为空，如纯图片消息） |
| `attachments` | `Attachment[]` | `[]` | 附件列表 |
| `tool_calls` | `ToolInvocation[]` | `[]` | 工具调用列表 |
| `thinking` | string | null | 思考过程（如 reasoning model 的思考链） |

### 5.3 Attachment

附件（文件 / 图片 / 视频 / 语音等非文本内容）。

| 字段 | 类型 | 说明 |
|------|------|------|
| `kind` | `"File"` / `"Image"` / `"Video"` / `"Voice"` | 附件类型 |
| `uri` | string | 引用路径（相对路径或外部 URL） |
| `mime_type` | string | MIME 类型（可选） |
| `size` | number | 大小（字节，可选） |

### 5.4 ToolInvocation

工具调用记录。

| 字段 | 类型 | 说明 |
|------|------|------|
| `name` | string | 工具名称（如 `WebSearch`） |
| `arguments` | string | 调用参数（JSON 字符串） |
| `result` | string | 调用结果（JSON 字符串） |
| `duration_ms` | number | 调用耗时（毫秒，可选） |

### 5.5 SummaryView

摘要钩子（轻量，用于注入 system prompt）。archive 和 summaries 端点返回此结构。

| 字段 | 类型 | 说明 |
|------|------|------|
| `hook_id` | string | 钩子 ID（UUID） |
| `memory_id` | string | 指向的记忆文件 ID |
| `summary_title` | string | 摘要标题 |
| `abstract_text` | string | 抽象摘要（2-3 句话，日级为空） |
| `key_facts` | string[] | 关键事实（事实级别，日级为空） |
| `key_entities` | string[] | 关键实体（人名 / 项目名 / 技术名词，日级为空） |
| `clue_anchors` | string[] | 线索锚点（月级才有） |
| `tags` | string[] | 标签集合（中文显示） |
| `archived_at` | RFC3339 | 归档时间 |
| `period` | string | 周期层级（`daily` / `weekly` / `monthly`） |
| `token_count` | number | Token 数 |

### 5.6 MemoryFile

完整记忆文件（一次归档的完整上下文）。retrieve 端点返回此结构。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID | 记忆文件唯一 ID |
| `schema_version` | number | Schema 版本 |
| `archived_at` | RFC3339 | 归档时间戳 |
| `session_id` | string | 所属会话 ID |
| `project_id` | string | 所属项目 ID（可选） |
| `turns` | `MessageTurn[]` | 该批次所有轮次（完整内容，非摘要） |
| `tags` | `Tag[]` | 标签集合（所有轮次标签的并集） |
| `total_tokens` | number | 总 token 数 |
| `truncated` | boolean | 是否被强制截断（超过 1.5 倍阈值时） |
| `period` | `"Daily"` / `"Weekly"` / `"Monthly"` | 归档周期层级 |
| `access_count` | number | 访问计数（评分维度之一） |
| `importance` | number | 用户显式重要性标记（0-100，默认 0） |
| `updates` | `MemoryUpdateRecord[]` | 记忆迭代更新历史 |

## 6. 项目隔离

MemoryCenter 支持通过 `project_id` 实现多项目隔离。同一 MemoryCenter 实例可服务多个项目，记忆文件按项目独立存储。

### 6.1 使用方式

| 端点类型 | 传参方式 | 字段 |
|---------|---------|------|
| POST 请求（archive / pre-compress / compaction / batch-* / search） | 请求体 | `project_id` |
| GET 请求（retrieve / summaries / prompt / conflicts） | 查询参数 | `?project_id=xxx` |

### 6.2 支持项目隔离的端点

| 端点 | 支持项目隔离 |
|------|-------------|
| POST `/archive` | 是（请求体 `project_id`） |
| POST `/pre-compress` | 是（请求体 `project_id`） |
| GET `/memories/{hook_id}` | 是（查询参数 `project_id`） |
| GET `/summaries` | 是（查询参数 `project_id`） |
| GET `/prompt` | 是（查询参数 `project_id`） |
| POST `/compaction` | 是（请求体 `project_id`） |
| PATCH `/memories/{hook_id}` | 是（请求体 `project_id`） |
| GET `/memories/{hook_id}/conflicts` | 是（查询参数 `project_id`） |
| POST `/memories/{hook_id}/detect-conflicts` | 是（请求体 `project_id`） |
| POST `/memories/batch-retrieve` | 是（请求体 `project_id`） |
| POST `/memories/batch-delete` | 是（请求体 `project_id`） |
| POST `/memories/batch-update` | 是（请求体 `project_id`） |
| POST `/search` | 否（session 级隔离，不含 project_id） |
| GET `/presets/*` | 否（无状态预设查询，与项目无关） |

### 6.3 存储路径

`project_id` 影响存储路径，未传时存入默认命名空间。示例路径结构：

```
{MEMORY_CENTER_ROOT}/
  sessions/{sid}/                          # 无 project_id
  projects/{project_id}/sessions/{sid}/    # 有 project_id
```

## 7. 错误处理

### 7.1 统一错误响应格式

所有错误响应使用统一的 JSON 结构：

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "人类可读的错误描述"
  }
}
```

### 7.2 错误码表

| HTTP 状态码 | code | 说明 | 触发场景 |
|------------|------|------|---------|
| 400 | `BAD_REQUEST` | 请求参数错误 | turns 为空 / period 值无效 / JSON 解析失败 / summary_template 缺少占位符 |
| 401 | `UNAUTHORIZED` | 未授权 | 未携带 Authorization 头 / 格式错误（需 `Bearer <key>`） |
| 403 | `FORBIDDEN` | 禁止访问 | API Key 不匹配 |
| 404 | `NOT_FOUND` | 资源未找到 | hook_id 不存在 / 记忆文件已删除 |
| 500 | `INTERNAL_ERROR` | 内部错误 | 归档失败 / 存储读写失败 / 序列化失败 |
| 501 | `NOT_IMPLEMENTED` | 功能未实现 | 语义检索未配置 Embedder API |

### 7.3 鉴权错误（v2.24+）

配置 `MEMORY_CENTER_API_KEY` 后，所有 REST API 请求需携带 `Authorization: Bearer <key>` 头。鉴权使用常量时间比对，避免时序侧信道攻击。

| 场景 | HTTP | code | 响应示例 |
|------|------|------|---------|
| 未携带 Authorization 头 | 401 | `UNAUTHORIZED` | `{"error":{"code":"UNAUTHORIZED","message":"缺少 Authorization 头"}}` |
| 格式错误（非 Bearer 前缀） | 401 | `UNAUTHORIZED` | `{"error":{"code":"UNAUTHORIZED","message":"Authorization 头格式错误，应为 'Bearer <api_key>'"}}` |
| API Key 不匹配 | 403 | `FORBIDDEN` | `{"error":{"code":"FORBIDDEN","message":"API Key 不正确"}}` |

## 8. 与 MCP Server 的关系

### 8.1 共享 Axum 服务（v2.36+）

从 v2.36 起，HTTP REST API 和 MCP Server 共享同一个 Axum 服务。设置 `MEMORY_CENTER_MCP_ENABLED=true` 后，`/mcp` 端点与 `/api/v1/*` 端点运行在同一进程、同一端口。

| 特性 | REST API (`/api/v1/*`) | MCP 端点 (`/mcp`) |
|------|------------------------|-------------------|
| 协议 | HTTP REST + JSON | JSON-RPC 2.0 over HTTP/SSE |
| 鉴权 | API Key（`Authorization: Bearer`） | MCP 协议自身认证（不经过 `require_api_key`） |
| 状态 | 无状态 | 可选 session 模式（`MEMORY_CENTER_MCP_STATEFUL`） |
| 方法 | GET / POST / PATCH | POST（请求）/ GET（SSE 流）/ DELETE（关闭 session） |
| 适用客户端 | 自定义集成 / 非 MCP 客户端 | 标准 Agent 客户端 |

### 8.2 何时用 REST API vs MCP Server

| 场景 | 推荐接口 | 理由 |
|------|---------|------|
| 自定义集成（Python/Node/Go 脚本） | REST API | 直接 HTTP 调用，无需 MCP SDK |
| 非 MCP 客户端（浏览器 / 移动端） | REST API | 标准 REST，任何 HTTP 客户端可用 |
| Claude Code / Cursor / Trae / Codex CLI | MCP Server | 标准 Agent 客户端，自动发现 21 个 tools |
| DeepSeek 网页端等远程 Agent | MCP Streamable HTTP | 远程访问，多客户端共享 |
| 需要细粒度控制（如批量操作） | REST API | REST 批量端点更灵活 |

### 8.3 MCP Tools 交叉引用

REST API 的端点与 MCP Server 的 21 个 tools 在能力上对等。下表列出对应关系。

| REST API 端点 | 对应 MCP Tool | 说明 |
|--------------|--------------|------|
| POST `/archive` | `archive` | 归档轮次 |
| POST `/pre-compress` | `pre_compress_hook` | 压缩前完整归档 |
| GET `/memories/{hook_id}` | `retrieve` | 检索记忆 |
| GET `/summaries` | `summaries` | 摘要列表 |
| GET `/prompt` | `prompt` | 渲染 system prompt |
| POST `/compaction` | `compaction` | 周期任务 |
| PATCH `/memories/{hook_id}` | -（MCP 端通过 `batch_update` 实现） | 更新记忆 |
| GET `/memories/{hook_id}/conflicts` | `get_conflicts` | 查询冲突记录 |
| POST `/memories/{hook_id}/detect-conflicts` | `detect_conflicts` | 冲突预检测 |
| POST `/memories/batch-retrieve` | `batch_retrieve` | 批量检索 |
| POST `/memories/batch-delete` | `batch_delete` | 批量删除 |
| POST `/memories/batch-update` | `batch_update` | 批量更新 |
| POST `/search` | `semantic_search` | 语义检索 |
| GET `/presets/agents` | `preset_list_agents` | 列出 Agent |
| GET `/presets/scenarios` | `preset_list_scenarios` | 列出 Scenario |
| GET `/presets/models` | `preset_list_models` | 列出 Model |
| POST `/presets/build` | `preset_build` | 构建预设 |
| - | `find_hook_by_prefix` | 按 hook_id 前缀查找（仅 MCP） |
| - | `get_config` | 查询运行时配置（仅 MCP） |
| - | `update_project_memory` | 更新 project_memory.md（仅 MCP） |
| - | `get_project_memory` | 读取 project_memory.md（仅 MCP） |
| - | `install_rules` | 规则安装（仅 MCP） |

> 6 个 tools（`find_hook_by_prefix` / `get_config` / `update_project_memory` / `get_project_memory` / `install_rules`）仅在 MCP 端提供，无对应 REST 端点。这是因为它们与 MCP 客户端的 IDE 集成场景强耦合。详见 [MCP Integration](MCP-Integration)。

## 9. 客户端示例

### 9.1 curl

```bash
# 设置基础变量
export MC_HOST=http://localhost:8765
export MC_API_KEY=your-api-key  # 配置了 MEMORY_CENTER_API_KEY 时需要

# 归档
curl -X POST "$MC_HOST/api/v1/sessions/sess-001/archive" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $MC_API_KEY" \
  -d '{"turns": [{"user_message": {"text": "你好"}, "llm_message": {"text": "你好！"}}]}'

# 获取摘要
curl -H "Authorization: Bearer $MC_API_KEY" \
  "$MC_HOST/api/v1/sessions/sess-001/summaries"

# 渲染 prompt
curl -H "Authorization: Bearer $MC_API_KEY" \
  "$MC_HOST/api/v1/sessions/sess-001/prompt"

# 检索记忆
curl -H "Authorization: Bearer $MC_API_KEY" \
  "$MC_HOST/api/v1/sessions/sess-001/memories/550e8400-..."

# 周期任务
curl -X POST "$MC_HOST/api/v1/sessions/sess-001/compaction" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $MC_API_KEY" \
  -d '{"period": "weekly"}'
```

### 9.2 Python（requests）

```python
import requests

BASE = "http://localhost:8765/api/v1"
HEADERS = {"Authorization": "Bearer your-api-key"}  # 未配置 API Key 时可省略
SID = "sess-001"

# 归档
resp = requests.post(f"{BASE}/sessions/{SID}/archive", headers=HEADERS, json={
    "turns": [
        {
            "user_message": {"text": "你好", "attachments": [], "tool_calls": [], "thinking": None},
            "llm_message": {"text": "你好！", "attachments": [], "tool_calls": [], "thinking": None},
            "token_count": 20,
        }
    ],
    "project_id": "proj-a",
})
summary = resp.json()
print(f"归档成功，hook_id={summary['hook_id']}")

# 获取 prompt
resp = requests.get(f"{BASE}/sessions/{SID}/prompt", headers=HEADERS)
print(resp.json()["prompt"])

# 语义检索
resp = requests.post(f"{BASE}/sessions/{SID}/search", headers=HEADERS, json={
    "query": "技术栈",
    "top_k": 3,
})
for hit in resp.json()["results"]:
    print(f"[{hit['score']:.2f}] {hit['summary_title']}")
```

### 9.3 JavaScript（fetch）

```javascript
const BASE = "http://localhost:8765/api/v1";
const HEADERS = {
  "Content-Type": "application/json",
  "Authorization": "Bearer your-api-key",  // 未配置 API Key 时可省略
};
const SID = "sess-001";

// 归档
const resp = await fetch(`${BASE}/sessions/${SID}/archive`, {
  method: "POST",
  headers: HEADERS,
  body: JSON.stringify({
    turns: [
      {
        user_message: { text: "你好", attachments: [], tool_calls: [], thinking: null },
        llm_message: { text: "你好！", attachments: [], tool_calls: [], thinking: null },
        token_count: 20,
      },
    ],
    project_id: "proj-a",
  }),
});
const summary = await resp.json();
console.log(`归档成功，hook_id=${summary.hook_id}`);

// 获取摘要
const resp2 = await fetch(`${BASE}/sessions/${SID}/summaries`, { headers: HEADERS });
const summaries = await resp2.json();
console.log(`共 ${summaries.length} 条记忆`);
```

## 10. 限制与注意事项

### 10.1 无状态设计

MemoryCenter HTTP 服务采用无状态设计，每次请求从磁盘读取、操作完释放。这意味着：

- 天然支持水平扩展（多实例无共享状态）
- 无连接池 / 会话保持开销
- 每次 `retrieve` / `summaries` / `prompt` 都会读取磁盘文件，IO 密集场景需考虑存储后端性能

### 10.2 性能考虑

| 场景 | 建议 |
|------|------|
| 大量归档（>100 轮/次） | 单次 archive 的 turns 数组不宜过大，建议分批归档 |
| 批量检索 | 使用 `/memories/batch-retrieve`（内置 8 并发）而非多次单条 retrieve |
| 语义检索 | 必须配置 `MEMORY_CENTER_EMBEDDER_*`，否则返回 501 |
| 周期任务 | 周级合并 / 月级淘汰会读写大量记忆文件，建议低峰期执行 |
| 搜索索引 | 归档后自动触发 `index_hook`，大量并发归档时索引构建可能成为瓶颈 |

### 10.3 推荐请求频率

| 操作 | 推荐频率 | 说明 |
|------|---------|------|
| `archive` | 达到 token 阈值时（非每轮） | 由 `archive_threshold` 控制，默认 120K tokens |
| `prompt` | 会话开始时 1 次 | 获取历史记忆摘要注入 system prompt |
| `summaries` | 会话开始时 1 次 | 了解有哪些历史记忆 |
| `retrieve` | LLM 需要历史细节时按需 | 不要预取，按 hook_id 精确检索 |
| `search` | 用户提到过去事件时 | 关键词触发，非每轮调用 |
| `compaction` | 周级每周 1 次 / 月级每月 1 次 | 低峰期执行 |

### 10.4 向后兼容

- 所有新增字段使用 `#[serde(default)]` 或 `Option<T>`，旧客户端请求不受影响
- 未配置 LLM 组件时自动降级为启发式实现，核心功能不受影响
- `MEMORY_CENTER_API_KEY` 未配置时跳过鉴权（本地开发零配置可用）

### 10.5 安全建议

| 场景 | 建议 |
|------|------|
| 生产环境 | 必须配置 `MEMORY_CENTER_API_KEY`，避免未授权访问 |
| 公网暴露 | 配置 `MEMORY_CENTER_MCP_ALLOWED_HOSTS` / `MEMORY_CENTER_MCP_ALLOWED_ORIGINS` 限制访问来源 |
| 反向代理 | 建议通过 Nginx 等反向代理暴露，增加 TLS / 限流 / IP 白名单 |
| API Key 管理 | 使用强随机字符串，定期轮换，不硬编码到客户端代码 |

## 下一步

- [MCP Integration](MCP-Integration) —— 通过 MCP Server 接入 Claude Code / Cursor / Trae 等 AI 编程客户端
- [Deployment](Deployment) —— 生产环境部署配置（含 LLM 组件 / 反向代理 / 系统服务）
- [Crate Guide](Crate-Guide) —— 选择合适的 Crate（core / ffi / server / mcp / python / wasm）
