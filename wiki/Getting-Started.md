# Getting Started

本指南帮助你在 5 分钟内完成 MemoryCenter 的构建和基本调用。

## 前置要求

- Rust 1.85+（`rustup show` 确认版本）
- Git
- （可选）Python 3.8+ + maturin（Python 绑定）
- （可选）Node.js 18+（WASM 测试）

## 1. 克隆与构建

```bash
git clone https://github.com/lingtian303/MemoryCenter.git
cd MemoryCenter

# 全量构建（所有 crate）
cargo build --release

# 或只构建核心库 + C ABI
cargo build --release -p memory-center-ffi
```

构建产物：
- `target/release/libmemory_center.so`（Linux）
- `target/release/memory_center.dll`（Windows）
- `target/release/libmemory_center.dylib`（macOS）

## 2. C 调用示例

```c
#include "memory_center.h"
#include <stdio.h>

int main(void) {
    /* 1. 创建句柄 */
    MemoryCenterHandle* h = memory_center_new(
        "./mem_data",       /* 存储根目录 */
        "session-001",      /* 会话 ID */
        NULL                /* project_id */
    );
    if (!h) { return 1; }

    /* 2. 归档轮次 */
    const char* turns_json = /* MessageTurn 数组 JSON */;
    MemoryCenterResult* r = memory_center_archive(h, turns_json);
    if (memory_center_is_ok(r)) {
        char* data = memory_center_get_data(r);
        printf("归档成功：%s\n", data);
        memory_center_free_string(data);
    }
    memory_center_result_free(r);

    /* 3. 渲染 prompt */
    MemoryCenterResult* pr = memory_center_render_prompt(h);
    if (memory_center_is_ok(pr)) {
        char* prompt = memory_center_get_data(pr);
        /* 拼接到 LLM system prompt */
        memory_center_free_string(prompt);
    }
    memory_center_result_free(pr);

    /* 4. 释放 */
    memory_center_free(h);
    return 0;
}
```

编译：`gcc demo.c -L. -lmemory_center -o demo`

## 3. Python 调用（推荐）

```bash
cd crates/memory-center-python
pip install maturin
maturin develop --release
```

```python
from memory_center_python import MemoryCenter

with MemoryCenter("./mem_data", "session-001", project_id="proj-a") as hp:
    # 归档
    summary = hp.archive([
        {
            "user_message": {"text": "你好", "attachments": [], "tool_calls": [], "thinking": None},
            "llm_message": {"text": "你好！有什么可以帮你？", "attachments": [], "tool_calls": [], "thinking": None},
            "tags": [{"kind": "Text"}],
            "token_count": 20,
        }
    ])
    print(f"归档成功，hook_id={summary['hook_id']}")

    # 获取摘要
    summaries = hp.summaries()
    print(f"共 {len(summaries)} 条记忆")

    # 渲染 prompt
    prompt = hp.prompt()
    if prompt:
        print(prompt)

    # 检索完整记忆
    memory = hp.retrieve(summary["hook_id"])
    print(f"检索到 {len(memory['turns'])} 轮对话")

    # 周期任务
    hp.compaction("weekly")   # 周级去重合并
    hp.compaction("monthly")  # 月级评分淘汰
```

## 4. HTTP REST API

```bash
# 启动服务
MEMORY_CENTER_HOST=0.0.0.0 MEMORY_CENTER_PORT=8765 MEMORY_CENTER_ROOT=./data \
  cargo run -p memory-center-server
```

```bash
# 归档
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/archive \
  -H "Content-Type: application/json" \
  -d '{"turns": [...], "project_id": "proj-a"}'

# 获取摘要
curl http://localhost:8765/api/v1/sessions/sess-001/summaries

# 渲染 prompt
curl http://localhost:8765/api/v1/sessions/sess-001/prompt
```

详见 [API Reference](API-Reference)。

## 5. MCP Server（推荐用于 AI 编程客户端）

```bash
cargo build --release -p memory-center-mcp
```

在 Claude Code / Cursor / Trae 的 MCP 配置中添加：

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

启动后 Agent 自动发现 21 个 tools。详见 [MCP Integration](MCP-Integration)。

## 6. 运行测试

```bash
# Rust 全量测试
cargo test --workspace

# Clippy 检查
cargo clippy --workspace --all-targets -- -D warnings

# Python 集成测试
cd crates/memory-center-python
maturin develop --release
pytest tests/test_memory_center.py -v
```

## 下一步

- [Architecture](Architecture) —— 理解三级周期和分层设计
- [Crate Guide](Crate-Guide) —— 选择合适的 Crate
- [MCP Integration](MCP-Integration) —— 接入 AI 编程客户端
- [Deployment](Deployment) —— 部署到生产环境
