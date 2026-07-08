# MemoryCenter

> Agent 记忆库依赖库 —— 跨语言可引用的持久化高效完整记忆系统

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/LINGTIAN303/MemoryCenter/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![CI](https://github.com/LINGTIAN303/MemoryCenter/actions/workflows/ci.yml/badge.svg)](https://github.com/LINGTIAN303/MemoryCenter/actions/workflows/ci.yml)
[![GitHub stars](https://img.shields.io/github/stars/LINGTIAN303/MemoryCenter)](https://github.com/LINGTIAN303/MemoryCenter/stargazers)

## 这是什么

MemoryCenter 是一个为 AI Agent 提供时序记忆管理的基础设施库。它完整保存对话上下文（非摘要），通过三级周期（天/周/月）管理记忆生命周期，支持 Rust / C / Python / MCP / WASM 多种接入方式。

**向量库做语义检索（找"像什么"），MemoryCenter 做时序归档（找"之前发生过什么"）——两者互补不替代。**

## 四个独家护城河

1. **三级索引周期 + 4 维加权评分淘汰**——天归档/周无损去重合并/月评分淘汰
2. **完整对话非摘要归档**——所有竞品都走压缩/抽取/摘要路径，MemoryCenter 无损保存可追溯
3. **Rust 单二进制 + C ABI 嵌入**——唯一可嵌入宿主进程的方案，零外部依赖
4. **17 类消息级标签**——粒度最细，支持按工具调用/思考过程/代码块等维度筛选

## 文档导航

| 我想... | 去哪看 |
|---------|--------|
| 快速上手试用 | [快速开始](getting-started.md) |
| 理解整体架构 | [整体架构](architecture.md) |
| 选择合适的 Crate | [Crate 选择指南](crate-guide.md) |
| 接入 MCP（Claude Code / Cursor / Trae） | [MCP 集成](mcp-integration.md) |
| 查看 REST API 文档 | [REST API 参考](api-reference.md) |
| 部署到生产环境 | [部署指南](deployment.md) |
| 查看版本变更 | [变更历史](changelog.md) |

## 链接

- **源码仓库**：[GitHub](https://github.com/LINGTIAN303/MemoryCenter)
- **GitHub Wiki**：[Wiki 镜像](https://github.com/LINGTIAN303/MemoryCenter/wiki)（与本 mdBook 内容同步）
- **Issue 反馈**：[Issues](https://github.com/LINGTIAN303/MemoryCenter/issues)
- **贡献指南**：[CONTRIBUTING.md](https://github.com/LINGTIAN303/MemoryCenter/blob/main/CONTRIBUTING.md)

## License

[MIT](https://github.com/LINGTIAN303/MemoryCenter/blob/main/LICENSE)
