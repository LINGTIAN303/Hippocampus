# MCP Integration

MemoryCenter MCP Server 让 AI Agent 通过标准协议（Model Context Protocol）调用记忆库能力。支持 21 个 tools，覆盖归档/检索/冲突检测/周期管理/预设查询等全链路操作。

## 传输模式

| 模式 | 版本 | 适用场景 | 配置 |
|------|------|----------|------|
| **stdio** | v2.3 | 本地 IDE（Claude Code / Cursor / Trae / Codex CLI） | `command` + `env` |
| **Streamable HTTP** | v2.36 | 远程客户端（DeepSeek 网页端等 Web Agent） | `url` + `transport` |

## stdio 模式配置

### 构建

```bash
cargo build --release -p memory-center-mcp
# 产物：target/release/memory-center-mcp
```

### Claude Code

在 `~/.claude/claude_desktop_config.json` 中添加：

```json
{
  "mcpServers": {
    "memory-center": {
      "command": "/path/to/memory-center-mcp",
      "env": {
        "MEMORY_CENTER_ROOT": "/path/to/memory/data"
      }
    }
  }
}
```

### Cursor

在 `.cursor/mcp.json` 中添加同样的配置。

### Trae

在项目的 `.mcp.json` 中添加：

```json
{
  "mcpServers": {
    "memory-center": {
      "command": "d:/path/to/memory-center-mcp.exe",
      "env": {
        "MEMORY_CENTER_ROOT": "d:/path/to/memory/data"
      }
    }
  }
}
```

## Streamable HTTP 模式配置

### 启动服务

```bash
MEMORY_CENTER_MCP_ENABLED=true \
MEMORY_CENTER_ROOT=./data \
MEMORY_CENTER_MCP_ALLOWED_HOSTS=your-domain.com \
cargo run -p memory-center-server
```

### 客户端配置

```json
{
  "mcpServers": {
    "memory-center": {
      "url": "https://your-domain.com/mcp",
      "transport": "streamable-http"
    }
  }
}
```

### 环境变量

| 环境变量 | 说明 | 默认值 |
|---------|------|--------|
| `MEMORY_CENTER_MCP_ENABLED` | 是否启用 MCP HTTP 端点 | `false` |
| `MEMORY_CENTER_MCP_STATEFUL` | 是否启用 session 模式（支持 SSE 流） | `true` |
| `MEMORY_CENTER_MCP_ALLOWED_HOSTS` | 允许的 Host（DNS rebinding 防护） | `localhost,127.0.0.1,::1` |
| `MEMORY_CENTER_MCP_ALLOWED_ORIGINS` | 允许的 Origin（CORS 防护） | 空 |

## 21 个 MCP Tools

### 归档/检索

| Tool | 说明 |
|------|------|
| `archive` | 归档一批轮次为记忆文件，生成索引钩子 |
| `pre_compress_hook` | 压缩前一次性完整归档（raw_context + 解析 turns 双轨） |
| `retrieve` | 按 hook_id 检索完整记忆文件 |
| `batch_retrieve` | 批量检索多个记忆文件 |
| `batch_delete` | 批量删除记忆文件（软删除） |
| `batch_update` | 批量更新记忆文件（added/revised/deprecated facts） |
| `find_hook_by_prefix` | 按短 ID 前缀查找完整 hook_id（跨 session） |

### 摘要/渲染

| Tool | 说明 |
|------|------|
| `summaries` | 获取所有周期的记忆摘要列表 |
| `prompt` | 渲染摘要为 system prompt 文本 |
| `get_config` | 查询运行时配置快照（归档阈值 / Agent / 降级状态） |

### 检索增强

| Tool | 说明 |
|------|------|
| `semantic_search` | 语义检索（BM25 + Embedding 混合，含降级） |
| `detect_conflicts` | 检测记忆更新的潜在冲突（三维度） |
| `get_conflicts` | 查询指定记忆的冲突历史记录 |

### 周期任务

| Tool | 说明 |
|------|------|
| `compaction` | 触发周期任务（weekly 去重合并 / monthly 评分淘汰） |

### 预设管理

| Tool | 说明 |
|------|------|
| `preset_build` | 即时构建 CombinedProfile，返回最终生效值 |
| `preset_list_agents` | 列出 11 个内置 Agent |
| `preset_list_scenarios` | 列出 7 个内置 Scenario |
| `preset_list_models` | 列出所有 ModelVariant |

### 项目记忆

| Tool | 说明 |
|------|------|
| `update_project_memory` | 更新 project_memory.md 副本指定章节 |
| `get_project_memory` | 读取 project_memory.md 副本完整内容 |

### 规则安装

| Tool | 说明 |
|------|------|
| `install_rules` | 安装记忆协议规则到项目（catpaw/trae/claude-code） |

`install_rules` 支持两种模式：
- **本地模式**（路径存在）：server 直接写入文件
- **远程模式**（路径不存在）：返回模板让 LLM 用 Write 工具创建文件

## Agent 自识别

MemoryCenter 启动时会自动识别 Agent 客户端（通过 MCP `initialize` 请求的 `client_info`），并注入对应的使用协议到 `server_info.instructions`。

内置 11 个 Agent 预设：
- Claude Code / Cursor / Trae / Codex CLI
- Continue / Cline / Roo Code / Windsurf
- DeepSeek++ / Generic / Other

## install_rules 使用

首次接入 MemoryCenter 时，调用 `install_rules` 安装记忆协议规则：

```
install_rules(
  project_root="/path/to/project",
  client="trae",  // catpaw / trae / claude-code
  force=false     // 是否覆盖已有规则
)
```

- **trae**：创建 `.trae/rules/memory-center-archive.md`
- **catpaw**：创建 `.catpaw/rules/memory-center-archive.md`
- **claude-code**：追加到 `CLAUDE.md`（带标记区间）

## 降级说明

| 未配置 | 降级行为 |
|--------|----------|
| LLM 摘要生成器 | 启发式摘要（首条消息前 80 字符） |
| Embedder API | 仅 BM25 关键词检索 |
| LLM 冲突检测器 | 启发式纯算法（三维度检测） |
| Agent 客户端未识别 | 不注入 usage_protocol，依赖 AGENTS.md |

## 相关文档

- [AGENTS.md](https://github.com/lingtian303/MemoryCenter/blob/main/AGENTS.md) —— 完整记忆协议规则
- [API Reference](API-Reference) —— REST API 文档
- [Deployment](Deployment) —— 生产环境部署
