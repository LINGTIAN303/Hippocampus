# REST API 参考

> 本章节是 [GitHub Wiki: API Reference](https://github.com/LINGTIAN303/MemoryCenter/wiki/API-Reference) 的镜像。

## 接口概览

所有 REST API 路径前缀：`/api/v1`

| 操作 | Method | Path |
|------|--------|------|
| 归档 | POST | `/sessions/{sid}/archive` |
| 检索 | GET | `/sessions/{sid}/memories/{hook_id}` |
| 摘要列表 | GET | `/sessions/{sid}/summaries` |
| 渲染 prompt | GET | `/sessions/{sid}/prompt` |
| 周期任务 | POST | `/sessions/{sid}/compaction` |

## 示例

### 归档

```bash
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/archive \
  -H "Content-Type: application/json" \
  -d '{"turns": [...], "project_id": "proj-a"}'
```

### 获取摘要

```bash
curl http://localhost:8765/api/v1/sessions/sess-001/summaries
```

### 渲染 prompt

```bash
curl http://localhost:8765/api/v1/sessions/sess-001/prompt
```

### 检索记忆

```bash
curl http://localhost:8765/api/v1/sessions/sess-001/memories/<hook_id>
```

### 周期任务

```bash
curl -X POST http://localhost:8765/api/v1/sessions/sess-001/compaction \
  -H "Content-Type: application/json" -d '{"period": "weekly"}'
```

## 启动服务

```bash
MEMORY_CENTER_HOST=0.0.0.0 \
MEMORY_CENTER_PORT=8765 \
MEMORY_CENTER_ROOT=./data \
cargo run -p memory-center-server
```

## 完整文档

完整 API 文档（含请求/响应 schema、错误码、鉴权）见：
- [Wiki: API Reference](https://github.com/LINGTIAN303/MemoryCenter/wiki/API-Reference)
- [crates/memory-center-server/src/handlers.rs](https://github.com/LINGTIAN303/MemoryCenter/blob/main/crates/memory-center-server/src/handlers.rs)
