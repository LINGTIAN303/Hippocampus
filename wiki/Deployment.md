# Deployment

本页详细说明 MemoryCenter 在不同场景下的部署方式，从本地开发到生产环境全覆盖。

## 1. 概览

MemoryCenter 支持多种部署模式，可根据使用场景灵活选择：

- **本地嵌入**：将 `memory-center-ffi` 动态库直接嵌入宿主进程，无需独立服务
- **单机服务**：运行 `memory-center-server` 二进制，提供 HTTP REST API + MCP Streamable HTTP
- **生产部署**：systemd 守护进程 + Nginx 反向代理 + Git auto-deploy hook，适合多客户端共享

生产部署的核心组件架构如下：

```
公网请求 https://your-domain/api/v1/...  或  https://your-domain/mcp
    ↓
Nginx（80/443，反向代理 + SSL 终止 + SSE 流支持）
    ↓ proxy_pass http://127.0.0.1:8765
systemd 守护进程 memory-center.service
    ↓
/opt/memory-center/bin/memory-center-server（Rust 单二进制，约 9-10MB）
    ↓
/opt/memory-center/data/（SQLite + 文件树存储）
    ↑
Git auto-deploy（post-receive hook：push → 编译 → 重启）
```

## 2. 部署模式选择

| 模式 | 适用场景 | 核心组件 | 鉴权方式 | 多客户端 |
|------|---------|---------|---------|---------|
| 本地嵌入 | 单进程应用、桌面 Agent、嵌入式设备 | `memory-center-ffi` + `memory-center-core` | 无需（进程内调用） | 否（单进程） |
| 单机服务 | 开发测试、局域网内共享、CI 环境 | `memory-center-server` | 可选 API Key | 是（HTTP） |
| 生产部署 | 公网多客户端共享、Web Agent 接入 | systemd + Nginx + Git hook | API Key + HTTPS | 是（HTTP + MCP） |

选择建议：
- 若你的 Agent 是 Rust/C/Python 单进程应用，优先选「本地嵌入」，零网络开销
- 若需要远程访问但不在意高可用，选「单机服务」
- 若需公网暴露、多客户端共享、自动部署，选「生产部署」

## 3. 前置要求

### 编译环境

| 组件 | 版本要求 | 说明 |
|------|---------|------|
| Rust | 1.85+ | `rustup show` 确认版本（edition 2021，rmcp 1.8 要求） |
| Git | 任意 | 拉取代码 |
| Cargo | 随 Rust 安装 | 构建工具 |

### 运行时环境

| 组件 | 版本要求 | 必要性 | 说明 |
|------|---------|--------|------|
| SQLite | 3.35+ | 必填（使用 SQLite 存储时） | 支持 RETURNING 子句 |
| Nginx | 1.18+ | 可选（生产部署） | 反向代理 + SSL 终止 |
| systemd | 任意 | 可选（Linux 生产） | 进程守护 |
| OpenSSL | 1.1+ | 可选 | 生成 API Key |

### 可选增强组件

| 组件 | 用途 | 未配置时行为 |
|------|------|------------|
| Embedder API（`MEMORY_CENTER_EMBEDDER_*`） | 语义检索（向量 + BM25 混合） | 降级为仅 BM25 关键词检索 |
| LLM API（`MEMORY_CENTER_GENERATOR_*`） | LLM 摘要生成 | 降级为启发式摘要（首条消息前 80 字符） |
| LLM 冲突检测器（`MEMORY_CENTER_DETECTOR_*`） | 三维度冲突检测 | 降级为启发式纯算法 |

## 4. 编译构建

### 4.1 克隆仓库

```bash
# 克隆代码
git clone https://github.com/lingtian303/MemoryCenter.git
cd MemoryCenter
```

### 4.2 构建 HTTP + MCP 服务

```bash
# 构建 HTTP REST + MCP Streamable HTTP 服务
cargo build --release -p memory-center-server

# 产物路径：target/release/memory-center-server（约 9-10MB）
```

### 4.3 构建 stdio MCP 二进制（可选）

```bash
# 构建 stdio 模式 MCP server（本地 IDE 用，如 Claude Code / Cursor / Trae）
cargo build --release -p memory-center-mcp

# 产物路径：target/release/memory-center-mcp
```

### 4.4 构建 C ABI 动态库（可选，本地嵌入模式用）

```bash
# 构建动态库
cargo build --release -p memory-center-ffi

# 产物路径：
#   Linux:   target/release/libmemory_center.so
#   Windows: target/release/memory_center.dll
#   macOS:   target/release/libmemory_center.dylib
```

### 4.5 交叉编译（可选）

若本地为 Windows/macOS，目标为 Linux 服务器，可在服务器上直接编译（推荐），或交叉编译：

```bash
# 添加 Linux x86_64 目标
rustup target add x86_64-unknown-linux-gnu

# 交叉编译
cargo build --release --target x86_64-unknown-linux-gnu -p memory-center-server

# 上传到服务器
scp target/x86_64-unknown-linux-gnu/release/memory-center-server \
  root@your-server:/opt/memory-center/bin/
```

> 交叉编译需要 `x86_64-linux-gnu-gcc`，Windows 上配置较复杂，推荐服务器直接编译。

## 5. 单机服务部署（开发/测试）

适合本地开发、CI 测试、局域网内共享。

### 5.1 环境变量配置

| 环境变量 | 说明 | 默认值 | 必填 |
|---------|------|--------|------|
| `MEMORY_CENTER_ROOT` | 存储根目录（SQLite + 文件树） | `./data` | 是 |
| `MEMORY_CENTER_HOST` | 监听地址 | `127.0.0.1` | 否 |
| `MEMORY_CENTER_PORT` | 监听端口 | `8765` | 否 |
| `MEMORY_CENTER_API_KEY` | API Key 鉴权 | 空（不鉴权） | 生产必填 |
| `MEMORY_CENTER_MCP_ENABLED` | 启用 MCP Streamable HTTP 端点 | `false` | 否 |
| `RUST_LOG` | 日志级别 | `memory_center_server=info,tower_http=info` | 否 |

### 5.2 启动服务

```bash
# 方式一：直接 cargo run（开发调试）
MEMORY_CENTER_HOST=0.0.0.0 \
MEMORY_CENTER_PORT=8765 \
MEMORY_CENTER_ROOT=./data \
MEMORY_CENTER_MCP_ENABLED=true \
  cargo run -p memory-center-server

# 方式二：运行 release 二进制（推荐）
MEMORY_CENTER_HOST=0.0.0.0 \
MEMORY_CENTER_PORT=8765 \
MEMORY_CENTER_ROOT=./data \
MEMORY_CENTER_MCP_ENABLED=true \
  ./target/release/memory-center-server
```

### 5.3 健康检查

```bash
# 测试摘要端点（无需鉴权时可省略 Authorization 头）
curl -sS -o /dev/null -w "HTTP %{http_code}\n" \
  http://127.0.0.1:8765/api/v1/sessions/probe/summaries

# 期望输出：HTTP 200
```

### 5.4 端到端功能测试

仓库内置测试脚本，覆盖归档/检索/摘要/Prompt/反代 5 个端点：

```bash
# 在服务器上执行
python3 deploy/test_e2e.py

# 或运行完整能力测试
python3 deploy/test_full_capabilities.py
```

> 测试脚本默认访问 `http://127.0.0.1:8765`（本地直连）。若配置了 `MEMORY_CENTER_API_KEY`，需在脚本中加上 `Authorization` 头。

## 6. 生产部署（systemd + Nginx）

### 6.1 目录规划

```
/opt/memory-center/
├── bin/
│   └── memory-center-server      # Rust 二进制
├── data/                          # 存储根目录（MEMORY_CENTER_ROOT）
│   ├── sessions/                  # 各 session 的记忆文件
│   │   └── {session_id}/
│   │       ├── hooks/             # 索引钩子
│   │       ├── memories/          # 完整记忆文件
│   │       └── raw_contexts/      # 压缩前归档的原始上下文
│   ├── projects/                  # 项目级记忆
│   │   └── {project_id}/
│   │       └── project_memory.md  # 项目记忆副本
│   └── memory_center.db           # SQLite 数据库（如启用）
└── logs/                          # （可选）日志目录

/root/MemoryCenter-work/            # Git checkout 工作目录（auto-deploy 用）
/root/memory-center.git/            # 裸仓库（auto-deploy 用）
```

### 6.2 部署二进制

```bash
# 创建目录
mkdir -p /opt/memory-center/bin /opt/memory-center/data

# 方式 A：服务器直接编译（推荐）
git clone https://github.com/lingtian303/MemoryCenter.git /root/MemoryCenter-work
cd /root/MemoryCenter-work
cargo build --release -p memory-center-server
cp target/release/memory-center-server /opt/memory-center/bin/

# 方式 B：上传预编译二进制
scp target/release/memory-center-server root@your-server:/opt/memory-center/bin/
```

### 6.3 systemd 服务配置

创建 `/etc/systemd/system/memory-center.service`：

```ini
[Unit]
Description=MemoryCenter Memory Service
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/memory-center
Environment=MEMORY_CENTER_HOST=127.0.0.1
Environment=MEMORY_CENTER_PORT=8765
Environment=MEMORY_CENTER_ROOT=/opt/memory-center/data
# 生产环境必须配置 API Key（用 openssl rand -hex 32 生成）
Environment=MEMORY_CENTER_API_KEY=请替换为你的强随机API Key
# 启用 MCP Streamable HTTP 端点（v2.36+）
Environment=MEMORY_CENTER_MCP_ENABLED=true
# MCP session 模式（true: 支持 SSE 流 + session 管理）
Environment=MEMORY_CENTER_MCP_STATEFUL=true
# DNS rebinding 防护：允许的 Host 列表
Environment=MEMORY_CENTER_MCP_ALLOWED_HOSTS=localhost,127.0.0.1,::1,your-domain.com
# CORS 防护：允许的 Origin 列表（逗号分隔）
Environment=MEMORY_CENTER_MCP_ALLOWED_ORIGINS=https://your-domain.com
# 日志级别
Environment=RUST_LOG=memory_center_server=info,tower_http=info
ExecStart=/opt/memory-center/bin/memory-center-server
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

> 仓库提供模板文件 `deploy/memory-center-server.service.template`，可复制后修改路径和 API Key。

生成强随机 API Key：

```bash
openssl rand -hex 32
# 输出示例：a3f5e8b2c1d4...（64 个十六进制字符）
```

### 6.4 启用服务

```bash
# 复制 systemd unit 文件（如使用模板）
sudo cp deploy/memory-center-server.service.template /etc/systemd/system/memory-center.service

# 或手动创建后，重新加载 systemd 配置
sudo systemctl daemon-reload

# 设置开机自启 + 立即启动
sudo systemctl enable memory-center
sudo systemctl start memory-center

# 查看服务状态
sudo systemctl status memory-center --no-pager -l

# 期望输出：Active: active (running)
```

### 6.5 Nginx 反向代理配置

Nginx 负责 SSL 终止、路径转发、SSE 流支持。以下为完整配置示例（含 REST API + MCP Streamable HTTP）：

```nginx
server {
    listen 443 ssl http2;
    server_name memory.example.com;

    # SSL 配置（用 Let's Encrypt 或自签证书）
    ssl_certificate /etc/letsencrypt/live/memory.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/memory.example.com/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;

    # REST API 反代
    # 公网路径：https://memory.example.com/api/v1/...
    location /api/ {
        proxy_pass http://127.0.0.1:8765;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 60s;
        proxy_send_timeout 60s;
    }

    # MCP Streamable HTTP 端点反代（v2.36+）
    # 公网路径：https://memory.example.com/mcp
    # SSE 流支持：proxy_buffering off + HTTP/1.1 + Connection 清空 + 长超时
    location /mcp {
        proxy_pass http://127.0.0.1:8765;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header Connection "";
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        # SSE 必需：关闭缓冲，实时转发事件流
        proxy_buffering off;
        proxy_cache off;
        # SSE 长连接超时（24h，与 WebSocket 一致）
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
    }
}

# HTTP → HTTPS 重定向
server {
    listen 80;
    server_name memory.example.com;
    return 301 https://$host$request_uri;
}
```

> 若 MemoryCenter 与其他服务共用一个域名，可将上述 `location` 块合并到现有 server 配置中。仓库提供独立配置文件 `deploy/nginx-memory-center.conf`（仅 REST API）和 `deploy/openworld_nginx_with_mcp.conf`（含 MCP 的完整示例）。

### 6.6 子路径部署（可选）

若需将 MemoryCenter 挂载到子路径（如 `https://your-domain/hippo/`），使用以下配置：

```nginx
# MemoryCenter API 子路径反代
# 公网路径：https://your-domain/hippo/api/v1/...
# 内部转发到 127.0.0.1:8765，自动去除 /hippo 前缀
location /hippo/ {
    proxy_pass http://127.0.0.1:8765/;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_read_timeout 60s;
    proxy_send_timeout 60s;
}
```

> `proxy_pass` 末尾带 `/` 会自动去除 `/hippo` 前缀。注意：子路径模式下 MCP 端点也需对应调整。

### 6.7 验证 Nginx 配置并 reload

```bash
# 测试配置语法
sudo nginx -t

# 重新加载（不中断现有连接）
sudo nginx -s reload
```

### 6.8 验证部署

```bash
# 1. 本地直连测试（绕过 Nginx + 鉴权）
curl -sS -o /dev/null -w "HTTP %{http_code}\n" \
  http://127.0.0.1:8765/api/v1/sessions/probe/summaries
# 期望：HTTP 200

# 2. Nginx 反代测试（本地 + 鉴权）
curl -sS -o /dev/null -w "HTTP %{http_code}\n" \
  -H "Authorization: Bearer 你的API Key" \
  http://127.0.0.1/api/v1/sessions/probe/summaries
# 期望：HTTP 200

# 3. 公网 HTTPS 测试
curl -sS -o /dev/null -w "HTTP %{http_code}\n" \
  -H "Authorization: Bearer 你的API Key" \
  https://memory.example.com/api/v1/sessions/probe/summaries
# 期望：HTTP 200

# 4. 鉴权失败测试（未携带 Authorization 头 → 401）
curl -sS -w "HTTP %{http_code}\n" \
  https://memory.example.com/api/v1/sessions/probe/summaries
# 期望：HTTP 401

# 5. 错误 API Key 测试（→ 403）
curl -sS -w "HTTP %{http_code}\n" \
  -H "Authorization: Bearer wrong-key" \
  https://memory.example.com/api/v1/sessions/probe/summaries
# 期望：HTTP 403
```

## 7. MCP Streamable HTTP 部署（v2.36+）

MCP Streamable HTTP 让远程客户端（如 DeepSeek 网页端、Web Agent）通过 HTTPS 接入 MemoryCenter，无需本地安装二进制。

### 7.1 启用方式

在 systemd unit 或启动命令中设置环境变量：

```bash
# 启用 MCP 端点
MEMORY_CENTER_MCP_ENABLED=true

# （可选）session 模式：true 支持 SSE 流 + session 管理，false 无状态
MEMORY_CENTER_MCP_STATEFUL=true

# （可选）DNS rebinding 防护：允许的 Host 列表
MEMORY_CENTER_MCP_ALLOWED_HOSTS=localhost,127.0.0.1,::1,your-domain.com

# （可选）CORS 防护：允许的 Origin 列表
MEMORY_CENTER_MCP_ALLOWED_ORIGINS=https://your-domain.com
```

### 7.2 端点说明

| 端点 | 方法 | 说明 |
|------|------|------|
| `/mcp` | POST | MCP 请求（JSON-RPC 2.0） |
| `/mcp` | GET | SSE 流（server → client 推送） |
| `/mcp` | DELETE | 关闭 session |

> `/mcp` 端点不经过 REST API 的 API Key 鉴权，MCP 客户端使用 MCP 协议自身认证。DNS rebinding + CORS 由 `StreamableHttpServerConfig` 内部处理。

### 7.3 配置项详解

| 环境变量 | 说明 | 默认值 |
|---------|------|--------|
| `MEMORY_CENTER_MCP_ENABLED` | 是否启用 MCP Streamable HTTP 端点 | `false`（需显式启用） |
| `MEMORY_CENTER_MCP_STATEFUL` | 是否启用 session 模式 | `true` |
| `MEMORY_CENTER_MCP_ALLOWED_HOSTS` | 允许的 Host 列表（逗号分隔，DNS rebinding 防护） | `localhost,127.0.0.1,::1` |
| `MEMORY_CENTER_MCP_ALLOWED_ORIGINS` | 允许的 Origin 列表（逗号分隔，CORS 防护） | 空（不校验 Origin） |
| `MEMORY_CENTER_PRESET_AGENT` | HTTP 模式下的 Agent 预设（Layer 1 识别） | 空 |

> Agent 识别限制：rmcp `service_factory` 签名不支持传入 ClientInfo，HTTP 模式下 per-session 自动识别（Layer 2）失效。生产环境推荐在 systemd unit 设置 `MEMORY_CENTER_PRESET_AGENT`（如 `trae` / `cursor` / `claude-code`）。

### 7.4 远程客户端配置示例

DeepSeek 网页端、ChatGPT 等支持 MCP 的 Web 客户端配置：

```json
{
  "mcpServers": {
    "memory-center": {
      "url": "https://memory.example.com/mcp",
      "transport": "streamable-http"
    }
  }
}
```

接入后，客户端会自动发现 21 个 MCP tools（archive / retrieve / semantic_search / pre_compress_hook 等）。详细 tools 列表见 [MCP Integration](MCP-Integration)。

### 7.5 与 stdio 模式的对比

| 维度 | stdio 模式 | Streamable HTTP 模式 |
|------|-----------|---------------------|
| 适用场景 | 本地 IDE（Claude Code / Cursor / Trae） | 远程客户端、Web Agent、多客户端共享 |
| 二进制 | `memory-center-mcp` | `memory-center-server`（共享） |
| 启用方式 | 客户端 MCP 配置 `command` | `MEMORY_CENTER_MCP_ENABLED=true` |
| 鉴权 | 无需（进程间通信） | DNS rebinding + CORS 防护 |
| 端点 | 无（stdin/stdout） | `/mcp`（POST / GET / DELETE） |
| 多客户端 | 否（每客户端独立进程） | 是（共享 Axum 服务） |

## 8. Git Auto-Deploy（post-receive hook）

通过 Git bare 仓库 + post-receive hook 实现 `git push production main` 自动编译 + 重启服务。

### 8.1 服务器端配置（一次性）

在服务器上执行 `deploy/setup-auto-deploy.sh` 脚本，自动完成以下操作：

```bash
# 1. 下载脚本到服务器
scp deploy/setup-auto-deploy.sh root@your-server:/root/

# 2. 在服务器上执行
ssh root@your-server
chmod +x /root/setup-auto-deploy.sh
/root/setup-auto-deploy.sh
```

脚本执行内容：
1. 创建裸仓库 `/root/memory-center.git`
2. 创建 post-receive hook（编译 + 替换二进制 + 重启服务）
3. 创建工作目录 `/root/MemoryCenter-work`
4. 验证现有服务状态

### 8.2 post-receive hook 脚本

hook 脚本核心逻辑如下（完整版见 `deploy/post-receive.sh`）：

```bash
#!/bin/bash
# 触发条件：git push production main
# 流程：checkout → cargo build → stop → cp → start → verify
set -e
export PATH=/root/.cargo/bin:/usr/bin:/bin:/usr/local/bin:$PATH

GIT_DIR=/root/memory-center.git
WORK_DIR=/root/MemoryCenter-work
BIN_DIR=/opt/memory-center/bin

while read oldrev newrev ref; do
    if [ "$ref" = "refs/heads/main" ]; then
        echo "[deploy] 开始部署 MemoryCenter Server"
        echo "[deploy] commit: $newrev"

        # 1. checkout 到工作目录
        mkdir -p "$WORK_DIR"
        git --work-tree="$WORK_DIR" --git-dir="$GIT_DIR" checkout -f main
        cd "$WORK_DIR"

        # 2. 编译 release 二进制（约 5-10 分钟）
        echo "[deploy] 编译 memory-center-server..."
        cargo build --release -p memory-center-server

        # 3. 停止服务（二进制运行中无法直接覆盖）
        echo "[deploy] 停止 memory-center 服务..."
        systemctl stop memory-center || true

        # 4. 复制新二进制
        echo "[deploy] 复制二进制..."
        mkdir -p "$BIN_DIR"
        cp target/release/memory-center-server "$BIN_DIR/"

        # 5. 启动服务
        echo "[deploy] 启动 memory-center 服务..."
        systemctl start memory-center

        # 6. 验证（等待 2 秒后检查状态）
        sleep 2
        if systemctl is-active --quiet memory-center; then
            echo "[deploy] 部署成功"
        else
            echo "[deploy] 错误：服务启动失败"
            systemctl status memory-center --no-pager | tail -20
            exit 1
        fi
    fi
done
```

### 8.3 本地配置 remote

```bash
# 在本地仓库添加 production remote
git remote add production root@your-server:/root/memory-center.git

# 验证
git remote -v
# production  root@your-server:/root/memory-center.git (fetch)
# production  root@your-server:/root/memory-center.git (push)
```

### 8.4 标准部署命令

本地与远端历史一致时，一行命令完成部署：

```bash
git add . && git commit -m "feat(xxx): 描述" && git push production main
```

push 后在服务器上观察部署日志：

```bash
# 实时查看 hook 输出（push 时的 SSH 输出会显示部署进度）
# 或在服务器上查看服务状态
sudo systemctl status memory-center --no-pager -l
```

### 8.5 历史分叉时的处理

若 push 报 `Updates were rejected because the remote contains work`：

```bash
# 方案一：rebase（推荐）
git fetch production main
git rebase production/main
git push production main

# 方案二：format-patch 打包上传（rebase 失败时）
git format-patch production/main -o /tmp/patches/
scp -r /tmp/patches root@your-server:/root/patches
ssh root@your-server
cd /root/MemoryCenter-work
git am /root/patches/*.patch
git push origin main --force  # 触发 hook
```

> 禁止使用 `git push --force` + 大文件 bundle/scp，容易导致 SSH 被 reset 且不触发 hook。

## 9. 数据备份

### 9.1 存储目录结构

`$MEMORY_CENTER_ROOT` 目录结构：

```
$MEMORY_CENTER_ROOT/
├── sessions/                          # 按 session 隔离
│   ├── {session_id}/
│   │   ├── hooks/                     # 索引钩子（JSON）
│   │   ├── memories/                  # 完整记忆文件（JSON）
│   │   ├── raw_contexts/              # 压缩前归档的原始上下文
│   │   └── session_state.json         # 会话状态快照
│   └── ...
├── projects/                          # 项目级记忆
│   └── {project_id}/
│       └── project_memory.md          # 项目记忆副本（反向写入）
├── memory_center.db                   # SQLite 数据库（如启用）
└── memory_center.db-wal               # SQLite WAL 文件
```

### 9.2 备份策略

```bash
# 手动备份：压缩整个存储目录
tar -czf memory-center-backup-$(date +%Y%m%d).tar.gz /opt/memory-center/data/

# 定时备份：crontab 每日凌晨 3 点执行
# 编辑 crontab
crontab -e

# 添加以下行（注意转义 %）
0 3 * * * tar -czf /backup/memory-center-$(date +\%Y\%m\%d).tar.gz /opt/memory-center/data/

# 保留最近 30 天备份（清理旧备份）
0 4 * * * find /backup/ -name "memory-center-*.tar.gz" -mtime +30 -delete
```

### 9.3 SQLite 备份（如启用 SQLite 存储）

```bash
# 使用 sqlite3 在线备份（不停服）
sqlite3 /opt/memory-center/data/memory_center.db ".backup /backup/memory_center-$(date +%Y%m%d).db"

# 或使用 VACUUM INTO（SQLite 3.27+）
sqlite3 /opt/memory-center/data/memory_center.db "VACUUM INTO '/backup/memory_center-$(date +%Y%m%d).db'"
```

### 9.4 恢复流程

```bash
# 1. 停止服务
sudo systemctl stop memory-center

# 2. 备份当前损坏的数据（以防万一）
mv /opt/memory-center/data /opt/memory-center/data.corrupted

# 3. 解压备份
tar -xzf /backup/memory-center-20260708.tar.gz -C /opt/memory-center/

# 4. 修复权限
chown -R root:root /opt/memory-center/data

# 5. 启动服务
sudo systemctl start memory-center

# 6. 验证
curl -sS -o /dev/null -w "HTTP %{http_code}\n" \
  -H "Authorization: Bearer 你的API Key" \
  http://127.0.0.1:8765/api/v1/sessions/probe/summaries
```

## 10. 监控与日志

### 10.1 查看日志

```bash
# 实时日志（跟踪模式）
journalctl -u memory-center -f

# 最近 100 行
journalctl -u memory-center -n 100 --no-pager

# 按时间筛选
journalctl -u memory-center \
  --since "2026-07-08 10:00" \
  --until "2026-07-08 12:00"

# 按关键字筛选
journalctl -u memory-center | grep "ERROR"

# 查看服务启动失败的详细错误
journalctl -u memory-center -n 50 --no-pager
```

### 10.2 日志级别调整

通过 `RUST_LOG` 环境变量控制日志级别：

```bash
# 默认（info）
RUST_LOG=memory_center_server=info,tower_http=info

# 调试模式（更详细）
RUST_LOG=memory_center_server=debug,tower_http=debug

# 仅警告和错误（生产环境降噪）
RUST_LOG=memory_center_server=warn,tower_http=warn

# 完全静默
RUST_LOG=off
```

修改后需重启服务：`sudo systemctl restart memory-center`

### 10.3 推荐监控指标

| 指标 | 获取方式 | 告警阈值建议 |
|------|---------|------------|
| 服务状态 | `systemctl is-active memory-center` | 非 active 立即告警 |
| 归档频率 | 日志中 `archive` 调用次数 | 突然归零或暴增异常 |
| 检索延迟 | `tower_http` 日志中的请求耗时 | P99 > 1s |
| 存储增长 | `du -sh /opt/memory-center/data/` | 日增长 > 500MB 异常 |
| 内存占用 | `systemctl show memory-center -p MainMemory` | > 1GB 需关注 |
| 磁盘剩余 | `df -h /opt/memory-center/` | < 20% 告警 |
| MCP 连接数 | 日志中 `/mcp` 请求频率 | 突然归零可能服务不可达 |

### 10.4 简易监控脚本

```bash
#!/bin/bash
# monitor-memory-center.sh
# 检查服务状态 + 存储大小 + 磁盘剩余

# 1. 服务状态
if systemctl is-active --quiet memory-center; then
    echo "[OK] 服务运行中"
else
    echo "[ALERT] 服务未运行！"
    systemctl status memory-center --no-pager | tail -10
fi

# 2. 存储大小
DATA_SIZE=$(du -sh /opt/memory-center/data/ | cut -f1)
echo "[INFO] 存储占用: $DATA_SIZE"

# 3. 磁盘剩余
DISK_FREE=$(df -h /opt/memory-center/ | awk 'NR==2 {print $5}' | tr -d '%')
if [ "$DISK_FREE" -gt 80 ]; then
    echo "[ALERT] 磁盘使用率 ${DISK_FREE}%，超过 80%"
else
    echo "[OK] 磁盘使用率 ${DISK_FREE}%"
fi

# 4. 健康检查
HTTP_CODE=$(curl -sS -o /dev/null -w "%{http_code}" \
  -H "Authorization: Bearer $MEMORY_CENTER_API_KEY" \
  http://127.0.0.1:8765/api/v1/sessions/probe/summaries)
if [ "$HTTP_CODE" = "200" ]; then
    echo "[OK] HTTP 健康检查通过"
else
    echo "[ALERT] HTTP 健康检查失败: $HTTP_CODE"
fi
```

加入 crontab 每 5 分钟检查一次：

```bash
*/5 * * * * /opt/memory-center/monitor-memory-center.sh >> /var/log/memory-center-monitor.log 2>&1
```

## 11. 升级与回滚

### 11.1 升级流程

#### 方式 A：Git auto-deploy（推荐，已配置 hook 时）

```bash
# 本地一行命令
git add . && git commit -m "feat(xxx): 升级到 vX.XX" && git push production main

# 服务器自动执行：checkout → cargo build → stop → cp → start → verify
```

#### 方式 B：手动升级

```bash
# 1. 拉取最新代码
cd /root/MemoryCenter-work
git pull origin main

# 2. 重新编译
cargo build --release -p memory-center-server

# 3. 停止服务
sudo systemctl stop memory-center

# 4. 备份旧二进制（便于回滚）
cp /opt/memory-center/bin/memory-center-server \
   /opt/memory-center/bin/memory-center-server.bak

# 5. 替换二进制
cp target/release/memory-center-server /opt/memory-center/bin/

# 6. 启动服务
sudo systemctl start memory-center

# 7. 验证
sudo systemctl status memory-center --no-pager
curl -sS -o /dev/null -w "HTTP %{http_code}\n" \
  -H "Authorization: Bearer $MEMORY_CENTER_API_KEY" \
  http://127.0.0.1:8765/api/v1/sessions/probe/summaries
```

### 11.2 回滚流程

```bash
# 1. 停止服务
sudo systemctl stop memory-center

# 2. 回滚代码到指定版本
cd /root/MemoryCenter-work
git log --oneline -10          # 查看历史版本
git checkout <旧版本commit hash>

# 3. 重新编译
cargo build --release -p memory-center-server

# 4. 替换二进制（或使用备份的二进制）
cp target/release/memory-center-server /opt/memory-center/bin/
# 或直接恢复备份：cp /opt/memory-center/bin/memory-center-server.bak /opt/memory-center/bin/memory-center-server

# 5. 启动服务
sudo systemctl start memory-center

# 6. 验证
sudo systemctl status memory-center --no-pager
```

### 11.3 数据迁移注意事项

- **migration 文件不可修改**：已发布的 migration 脚本一旦执行就不可修改，只能新增 migration 文件
- **升级前务必备份数据**：`tar -czf /backup/pre-upgrade-$(date +%Y%m%d).tar.gz /opt/memory-center/data/`
- **跨大版本升级**：先阅读 [Changelog](Changelog) 中的破坏性变更说明，必要时先在测试环境验证
- **降级风险**：高版本数据库 schema 可能不兼容低版本代码，降级前需确认 migration 是否可逆

## 12. 安全加固

### 12.1 防火墙配置

仅暴露 80/443 端口，后端服务端口 8765 不对外：

```bash
# UFW（Ubuntu/Debian）
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow 22/tcp        # SSH
sudo ufw allow 80/tcp        # HTTP
sudo ufw allow 443/tcp       # HTTPS
sudo ufw enable

# 验证
sudo ufw status verbose
```

```bash
# firewalld（CentOS/RHEL）
sudo firewall-cmd --permanent --add-service=ssh
sudo firewall-cmd --permanent --add-service=http
sudo firewall-cmd --permanent --add-service=https
sudo firewall-cmd --reload
```

### 12.2 API Key 管理

```bash
# 1. 生成强随机 API Key（64 字符）
openssl rand -hex 32

# 2. 配置到 systemd unit
# Environment=MEMORY_CENTER_API_KEY=a3f5e8b2c1d4...

# 3. 重新加载 + 重启
sudo systemctl daemon-reload
sudo systemctl restart memory-center

# 4. 定期轮换（建议每 90 天）
# 修改 Environment 后执行 daemon-reload + restart
```

注意事项：
- 不要将 API Key 写入代码或提交到 Git
- 不要在日志中打印 API Key
- 仅通过环境变量或 systemd 配置传递
- 修改 API Key 后所有客户端需同步更新

### 12.3 MCP 端点访问控制

```bash
# DNS rebinding 防护：限制允许的 Host
MEMORY_CENTER_MCP_ALLOWED_HOSTS=localhost,127.0.0.1,::1,your-domain.com

# CORS 防护：限制允许的 Origin
MEMORY_CENTER_MCP_ALLOWED_ORIGINS=https://your-domain.com,https://trusted-app.com

# Nginx 层面限制 IP（仅允许内网）
location /mcp {
    # 仅允许指定 IP 段
    allow 10.0.0.0/8;
    allow 192.168.0.0/16;
    deny all;
    
    proxy_pass http://127.0.0.1:8765;
    # ... 其他配置
}
```

### 12.4 SSL/TLS 配置

```nginx
# 推荐 SSL 配置（Nginx）
ssl_protocols TLSv1.2 TLSv1.3;
ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
ssl_prefer_server_ciphers off;
ssl_session_cache shared:SSL:10m;
ssl_session_timeout 10m;

# HSTS（强制 HTTPS）
add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;

# 申请 Let's Encrypt 免费证书
# sudo certbot --nginx -d memory.example.com
```

### 12.5 fail2ban 集成

防止恶意 IP 暴力探测 API Key：

```ini
# /etc/fail2ban/jail.d/memory-center.conf
[memory-center-auth]
enabled = true
port = 80,443
filter = memory-center-auth
logpath = /var/log/nginx/access.log
maxretry = 5
findtime = 300
bantime = 3600

# 触发条件：5 分钟内 5 次 401/403
```

```ini
# /etc/fail2ban/filter.d/memory-center-auth.conf
[Definition]
failregex = ^<HOST>.*"(GET|POST|PUT|DELETE).*" 40[13] .*$
ignoreregex =
```

```bash
# 启用
sudo systemctl enable fail2ban
sudo systemctl start fail2ban

# 查看封禁状态
sudo fail2ban-client status memory-center-auth
```

## 13. 故障排查

### 13.1 常见问题速查表

| 症状 | 可能原因 | 解决方案 |
|------|---------|---------|
| 服务无法启动 | 端口被占用 | `lsof -i :8765` 查找占用进程，或改 `MEMORY_CENTER_PORT` |
| 服务无法启动 | 存储目录无权限 | `chown -R root:root /opt/memory-center/data` |
| 服务无法启动 | 二进制架构不对 | `uname -m` 确认是 x86_64，重新编译 |
| 服务无法启动 | 配置文件语法错误 | `journalctl -u memory-center -n 50` 查看详细错误 |
| Nginx 返回 Vue 首页 HTML | `location /api/` 被主站点 `location /` 兜底 | 确认 `/api/` 在 `/` 之前，`nginx -T \| grep location` 检查 |
| MCP 客户端连接失败 | `MEMORY_CENTER_MCP_ENABLED` 未设为 true | 检查 systemd unit 环境变量 |
| MCP 客户端连接失败 | Nginx SSE 流被缓冲 | 确认 `proxy_buffering off; proxy_cache off;` |
| MCP 客户端连接失败 | DNS rebinding 防护拦截 | 将客户端域名加入 `MEMORY_CENTER_MCP_ALLOWED_HOSTS` |
| MCP 客户端连接失败 | CORS 防护拦截 | 将客户端 Origin 加入 `MEMORY_CENTER_MCP_ALLOWED_ORIGINS` |
| 401 鉴权失败 | 未携带 Authorization 头 | `curl -H "Authorization: Bearer 你的Key"` |
| 403 鉴权失败 | API Key 不正确 | `systemctl show memory-center -p Environment \| grep API_KEY` 核对 |
| 归档失败 | turns_json 格式错误 | 检查 JSON 是否符合 MessageTurn 结构 |
| 归档失败 | 存储空间不足 | `df -h` 检查磁盘，清理或扩容 |
| 性能下降 | SQLite WAL 文件过大 | `sqlite3 memory_center.db "PRAGMA wal_checkpoint(TRUNCATE);"` |
| 性能下降 | session 记忆过多 | 执行 `compaction` 周期任务（weekly/monthly） |
| push 部署失败 | post-receive hook 执行失败 | SSH 查看 `journalctl -u memory-center` 或部署日志 |
| push 部署失败 | 历史分叉 | `git fetch production main && git rebase production/main` |

### 13.2 详细排查步骤

#### 服务无法启动

```bash
# 1. 查看详细错误日志
journalctl -u memory-center -n 50 --no-pager

# 2. 检查端口占用
ss -tlnp | grep 8765

# 3. 检查二进制是否存在且可执行
ls -la /opt/memory-center/bin/memory-center-server

# 4. 检查存储目录权限
ls -la /opt/memory-center/data/

# 5. 手动启动查看错误（绕过 systemd）
cd /opt/memory-center
MEMORY_CENTER_HOST=127.0.0.1 MEMORY_CENTER_PORT=8765 \
  MEMORY_CENTER_ROOT=/opt/memory-center/data \
  ./bin/memory-center-server
```

#### MCP 客户端连接失败

```bash
# 1. 确认 MCP 已启用
systemctl show memory-center -p Environment | grep MCP_ENABLED
# 期望：MEMORY_CENTER_MCP_ENABLED=true

# 2. 本地直连测试 MCP 端点
curl -sS -o /dev/null -w "HTTP %{http_code}\n" \
  -X POST http://127.0.0.1:8765/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","method":"initialize","id":1,"params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"probe","version":"1.0"}}}'
# 期望：HTTP 200

# 3. 公网测试
curl -sS -o /dev/null -w "HTTP %{http_code}\n" \
  -X POST https://your-domain/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","method":"initialize","id":1,"params":{...}}'

# 4. 检查 Nginx SSE 配置
nginx -T 2>/dev/null | grep -A10 "location /mcp"
# 确认：proxy_buffering off; proxy_cache off; proxy_read_timeout 86400s;

# 5. 检查 DNS rebinding 防护
systemctl show memory-center -p Environment | grep ALLOWED_HOSTS
# 确认客户端访问的域名在允许列表中
```

#### PowerShell 上传 Nginx 配置后变量丢失

**原因**：PowerShell 反引号转义会吃掉 `$host`、`$remote_addr` 等 Nginx 变量。

**解决**：用本地文件 + `scp` 上传，避免通过 SSH 命令行传递含 `$` 的内容。

```bash
# 本地保存为 nginx.conf，然后上传
scp nginx.conf root@your-server:/etc/nginx/conf.d/memory-center.conf
ssh root@your-server "nginx -t && nginx -s reload"
```

## 下一步

- [Changelog](Changelog) —— 查看版本变更历史
- [API-Reference](API-Reference) —— REST API 端点完整文档
- [MCP-Integration](MCP-Integration) —— MCP stdio + Streamable HTTP 接入指南
- [Architecture](Architecture) —— 理解三级周期与分层架构
