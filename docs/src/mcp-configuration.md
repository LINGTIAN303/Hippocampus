# MCP 配置指南

> 本章节是 [GitHub Wiki: MCP Configuration Guide](https://github.com/LINGTIAN303/MemoryCenter/wiki/MCP-Configuration-Guide) 的镜像。

## 配置总览

MemoryCenter MCP 支持两种传输模式 × 四种部署环境，组合出 8 种典型配置场景。

| 传输模式 | 本地直连 | Docker | Nginx 反代 | Cloudflare CDN |
|---------|---------|--------|-----------|----------------|
| stdio | ✅ 最简 | ✅ | N/A | N/A |
| Streamable HTTP | ✅ | ✅ | ✅ | ⚠️ 需 HTTPS |

## 详细配置文档

完整配置指南（含通用配置 / stdio / HTTP / Cloudflare / Docker / 环境变量参考 / 常见问题排查 / 配置检查清单）见 [Wiki: MCP Configuration Guide](https://github.com/LINGTIAN303/MemoryCenter/wiki/MCP-Configuration-Guide)。

## 关键环境变量速查

| 环境变量 | 说明 | 默认值 |
|---------|------|--------|
| `MEMORY_CENTER_ROOT` | 记忆数据存储根目录 | 必填 |
| `MEMORY_CENTER_MCP_ENABLED` | 启用 MCP Streamable HTTP 端点 | `false` |
| `MEMORY_CENTER_MCP_STATEFUL` | 启用 session 模式 | `true` |
| `MEMORY_CENTER_MCP_ALLOWED_HOSTS` | 允许的 Host（DNS rebinding 防护） | `localhost,127.0.0.1,::1` |
| `MEMORY_CENTER_MCP_ALLOWED_ORIGINS` | 允许的 Origin（CORS 防护） | 空 |
| `MEMORY_CENTER_PRESET_AGENT` | 预设 Agent（HTTP 模式推荐设置） | 空（自动识别） |
| `MEMORY_CENTER_LLM_API_BASE` | LLM API 地址（摘要生成用） | 空（降级启发式） |
| `MEMORY_CENTER_LLM_API_KEY` | LLM API Key | 空（降级启发式） |
| `MEMORY_CENTER_LLM_MODEL` | LLM 模型名 | 空（降级启发式） |
| `MEMORY_CENTER_EMBEDDER_API_BASE` | Embedding API 地址（语义检索用） | 空（降级 BM25） |
| `MEMORY_CENTER_EMBEDDER_API_KEY` | Embedding API Key | 空（降级 BM25） |

## 踩坑提示

1. **Cloudflare CDN 必须 HTTPS**：HTTP 请求会被 301 重定向到 HTTPS，MCP 客户端不处理重定向，导致 `Connection closed`
2. **Binary 名称带连字符**：`memory-center-mcp`（正确）vs `memorycenter-mcp`（错误）
3. **HTTP 模式 Agent 识别**：服务端无法获取 MCP ClientInfo，需通过 `MEMORY_CENTER_PRESET_AGENT` 环境变量指定

完整踩坑排查见 [Wiki: MCP Configuration Guide §常见问题排查](https://github.com/LINGTIAN303/MemoryCenter/wiki/MCP-Configuration-Guide#常见问题排查)。
