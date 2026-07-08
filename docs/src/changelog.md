# 变更历史

> 本章节是 [GitHub Wiki: Changelog](https://github.com/LINGTIAN303/MemoryCenter/wiki/Changelog) 的镜像，保持与 [CHANGELOG.md](https://github.com/LINGTIAN303/MemoryCenter/blob/main/CHANGELOG.md) 同步。

## 版本状态

- ✅ **MVP（P0-P5）**：核心库 + C ABI 动态库 + 文档 + 示例 + 跨语言测试 + 性能基准
- ✅ **v2.1**：HTTP/Axum REST API 服务（无状态，水平扩展）
- ✅ **v2.2**：Python 原生绑定（PyO3 + maturin，OOP 风格 + 上下文管理器）
- ✅ **v2.3**：MCP Server（rmcp，stdio 传输）+ 预设系统（11 Agent + 7 Scenario）+ 差异化定位文档
- ✅ **v2.3x**：冲突检测 + 压缩前归档（pre_compress_hook）+ 语义检索 + project_memory 反向写入 + install_rules 规则安装
- ✅ **v2.35**：WASM 组件（wasm-bindgen + MemoryStorage + JsStorage + MemoryCenterCore JS API）
- ✅ **v2.36**：MCP Streamable HTTP 传输（`/mcp` 端点，与 REST API 共享 Axum 服务）
- ✅ **v2.37**：install_rules 远程模式（HTTPS MCP 模式下返回模板让 LLM 用 Write 工具创建文件）
- 🚧 **v2.4 路线图**：Go/Java 绑定 + 语义检索增强（向量库集成）

## 详细变更

完整变更历史见 [CHANGELOG.md](https://github.com/LINGTIAN303/MemoryCenter/blob/main/CHANGELOG.md) 与 [Wiki: Changelog](https://github.com/LINGTIAN303/MemoryCenter/wiki/Changelog)。
