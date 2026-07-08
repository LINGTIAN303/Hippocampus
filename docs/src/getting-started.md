# 快速开始

> 本章节是 [GitHub Wiki: Getting Started](https://github.com/LINGTIAN303/MemoryCenter/wiki/Getting-Started) 的镜像。

## 1. 构建

```bash
# 克隆仓库
git clone https://github.com/lingtian303/MemoryCenter.git
cd MemoryCenter

# 构建动态库（memory_center.dll / libmemory_center.so / libmemory_center.dylib）
cargo build --release -p memory-center-ffi

# 构建产物位于：
#   Windows: target/release/memory_center.dll
#   Linux:   target/release/libmemory_center.so
#   macOS:   target/release/libmemory_center.dylib
```

## 2. 接入方式选择

| 场景 | 推荐方式 | 详见 |
|------|---------|------|
| AI 编程客户端（Claude Code / Cursor / Trae） | MCP Server (stdio) | [MCP 集成](mcp-integration.md) |
| 远程 Agent / Web 端 | MCP Streamable HTTP | [MCP 配置指南](mcp-configuration.md) |
| Python 项目 | Python 原生绑定 (PyO3) | [README 示例](https://github.com/LINGTIAN303/MemoryCenter#4-python-原生绑定推荐v22) |
| C/C++ 项目 | C ABI 动态库 | [README 示例](https://github.com/LINGTIAN303/MemoryCenter#2-c-调用示例) |
| Node.js 项目 | napi-rs 绑定 | [README 示例](https://github.com/LINGTIAN303/MemoryCenter#crate-矩阵) |
| Rust 项目 | 直接依赖 crate | [Crate 选择指南](crate-guide.md) |
| 浏览器 / Edge | WASM 组件 | [Crate 选择指南](crate-guide.md) |

## 3. MCP Server 一键配置（推荐）

最简接入：构建 MCP server 二进制，在客户端配置文件中指定 command 即可。

```bash
cargo build --release -p memory-center-mcp
# 产物：target/release/memory-center-mcp
```

客户端配置（Claude Code / Cursor / Trae）：

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

启动后 Agent 会自动发现 21 个 tools，无需额外编码。

## 4. 下一步

- 理解 [整体架构](architecture.md)
- 选择合适的 [Crate](crate-guide.md)
- 详细配置见 [MCP 配置指南](mcp-configuration.md)
