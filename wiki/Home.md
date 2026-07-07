# MemoryCenter

> Agent 记忆库依赖库 —— 跨语言可引用的持久化高效完整记忆系统

MemoryCenter 是一个为 AI Agent 提供时序记忆管理的基础设施库。它完整保存对话上下文（非摘要），通过三级周期（天/周/月）管理记忆生命周期，支持 Rust / C / Python / MCP / WASM 多种接入方式。

## 为什么需要 MemoryCenter

**向量库做语义检索（找"像什么"），MemoryCenter 做时序归档（找"之前发生过什么"）——两者互补不替代。**

当前 Agent 记忆方案的痛点：
- **上下文窗口有限**：长对话会超出窗口，旧轮次被丢弃
- **压缩丢失信息**：客户端压缩是摘要式的，原始内容不可追溯
- **无时序管理**：现有方案不区分近期/周度/月度记忆，无法按时间维度淘汰

MemoryCenter 解决这些问题：达到阈值时冻结完整对话上下文为记忆文件，通过三级周期管理记忆生命周期。

## 四个独家护城河

1. **三级索引周期 + 4 维加权评分淘汰** —— 天归档 / 周无损去重合并 / 月评分淘汰
2. **完整对话非摘要归档** —— 所有竞品都走压缩/抽取/摘要路径，MemoryCenter 无损保存可追溯
3. **Rust 单二进制 + C ABI 嵌入** —— 唯一可嵌入宿主进程的方案，零外部依赖
4. **21 类消息级标签** —— 粒度最细，支持按工具调用/思考过程/代码块等维度筛选

## 快速导航

| 我想... | 去哪看 |
|---------|--------|
| 快速上手试用 | [Getting Started](Getting-Started) |
| 理解整体架构 | [Architecture](Architecture) |
| 选择合适的 Crate | [Crate Guide](Crate-Guide) |
| 接入 MCP（Claude Code / Cursor / Trae） | [MCP Integration](MCP-Integration) |
| 查看 REST API 文档 | [API Reference](API-Reference) |
| 部署到生产环境 | [Deployment](Deployment) |
| 查看版本变更 | [Changelog](Changelog) |

## 核心特性

- **完整上下文归档**（非摘要）：达到阈值时冻结完整对话上下文为记忆文件
- **三级索引周期**：天级持续归档 / 周级无损去重合并 / 月级评分淘汰
- **混合检索**：摘要钩子注入 system prompt + BM25 关键词检索 + 语义检索（含降级）
- **21 个 MCP Tools**：archive / retrieve / semantic_search / detect_conflicts / pre_compress_hook 等
- **双传输模式**：MCP stdio（本地）+ Streamable HTTP（远程）
- **跨语言**：Rust + C ABI + Python + MCP + WASM
- **可插拔架构**：Storage / Scorer / Migrator trait 均可替换
- **冲突检测**：用户陈述与记忆矛盾时自动检测（三维度）
- **project_memory 反向写入**：让记忆主动流入 IDE 的注入上下文

## 技术栈

- **Rust 1.85+**（edition 2021）
- **rmcp 1.8**（MCP stdio + Streamable HTTP）
- **Axum 0.8** + tower-http 0.7
- **PyO3 0.29** + maturin
- **wasm-bindgen**（WASM 组件）
- **SQLite**（可选存储后端，含向量搜索）

## License

MIT OR Apache-2.0
