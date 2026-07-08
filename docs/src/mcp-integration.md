# MCP 集成

> 本章节是 [GitHub Wiki: MCP Integration](https://github.com/LINGTIAN303/MemoryCenter/wiki/MCP-Integration) 的镜像。

## MCP 是什么

MCP（Model Context Protocol）是 Anthropic 推出的 Agent 工具调用协议，主流 AI 编程客户端全支持。MemoryCenter MCP server 让 Agent 通过标准协议调用记忆库能力，无需自己实现归档/检索逻辑。

## 两种传输模式

| 模式 | 版本 | 适用场景 | Binary |
|------|------|----------|--------|
| **stdio** | v2.3 | 本地 IDE（Claude Code / Cursor / Trae） | `memory-center-mcp` |
| **Streamable HTTP** | v2.36 | 远程客户端（DeepSeek 网页端等） | `memory-center-server` |

## 21 个 Tools 一览

| 类别 | Tools |
|------|-------|
| 归档/检索 | `archive` / `pre_compress_hook` / `retrieve` / `batch_retrieve` / `batch_delete` / `batch_update` / `find_hook_by_prefix` |
| 摘要/渲染 | `summaries` / `prompt` / `get_config` |
| 检索增强 | `semantic_search` / `detect_conflicts` / `get_conflicts` |
| 周期任务 | `compaction` |
| 预设管理 | `preset_build` / `preset_list_agents` / `preset_list_scenarios` / `preset_list_models` |
| 项目记忆 | `update_project_memory` / `get_project_memory` |
| 规则安装 | `install_rules`（支持本地直接写入 + 远程模板模式） |

## 最简配置（stdio 模式）

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

## Streamable HTTP 模式

```json
{
  "mcpServers": {
    "memory-center": {
      "url": "https://your-server/mcp",
      "transport": "streamable-http"
    }
  }
}
```

## 详细配置

各环境详细配置、踩坑排查见 [MCP 配置指南](mcp-configuration.md)。

完整 MCP tools 实现见 [crates/memory-center-mcp/src/lib.rs](https://github.com/LINGTIAN303/MemoryCenter/blob/main/crates/memory-center-mcp/src/lib.rs)。
MCP 集成测试见 [crates/memory-center-mcp/tests/](https://github.com/LINGTIAN303/MemoryCenter/tree/main/crates/memory-center-mcp/tests)（56 个测试用例）。
