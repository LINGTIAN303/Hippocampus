#!/bin/bash
# MemoryCenter Demo 独立部署脚本（参赛专用）
# 端口 8766，数据目录 /opt/memory-center-demo/data/
# 不污染现有 memory-center 服务数据
#
# 安全要求：
#   - LLM API Key 通过环境变量传入，不再硬编码
#   - 使用专用用户 memory-center 运行，不以 root 身份运行
#
# 用法：
#   export LLM_API_KEY="sk-your-real-key-here"
#   bash setup-demo.sh
set -e

# 检查必需的环境变量
if [ -z "$LLM_API_KEY" ]; then
    echo "错误：请先设置 LLM_API_KEY 环境变量"
    echo "  export LLM_API_KEY=\"sk-your-real-key-here\""
    exit 1
fi

echo "=== [1/7] 创建专用用户 ==="
if ! id -u memory-center &>/dev/null; then
    useradd -r -s /sbin/nologin -d /opt/memory-center-demo memory-center
    echo "用户 memory-center 已创建"
else
    echo "用户 memory-center 已存在"
fi

echo "=== [2/7] 创建独立目录 ==="
mkdir -p /opt/memory-center-demo/bin
mkdir -p /opt/memory-center-demo/data
cp /opt/memory-center/bin/memory-center-server /opt/memory-center-demo/bin/
chmod +x /opt/memory-center-demo/bin/memory-center-server
chown -R memory-center:memory-center /opt/memory-center-demo
echo "二进制已复制"

echo "=== [3/7] 写入 systemd unit ==="
cat > /etc/systemd/system/memory-center-demo.service << 'UNIT_EOF'
[Unit]
Description=MemoryCenter Demo Service (TRAE Contest)
After=network.target

[Service]
Type=simple
User=memory-center
Group=memory-center
WorkingDirectory=/opt/memory-center-demo
# 安全加固
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ReadWritePaths=/opt/memory-center-demo/data
# 服务配置
Environment=MEMORY_CENTER_HOST=127.0.0.1
Environment=MEMORY_CENTER_PORT=8766
Environment=MEMORY_CENTER_ROOT=/opt/memory-center-demo/data
Environment=MEMORY_CENTER_API_KEY=trae-contest-demo-key-2026
Environment=MEMORY_CENTER_MCP_ENABLED=true
Environment=MEMORY_CENTER_MCP_STATEFUL=true
Environment=MEMORY_CENTER_MCP_ALLOWED_HOSTS=localhost,127.0.0.1
Environment=RUST_LOG=memory_center_server=info,tower_http=info

# LLM 摘要生成器配置（SiliconFlow + Qwen2.5-7B）
Environment=MEMORY_CENTER_GENERATOR_API_URL=https://api.siliconflow.cn/v1/chat/completions
Environment=MEMORY_CENTER_GENERATOR_API_KEY=__LLM_API_KEY__
Environment=MEMORY_CENTER_GENERATOR_MODEL=Qwen/Qwen2.5-7B-Instruct
Environment=MEMORY_CENTER_GENERATOR_TIMEOUT=60
Environment=MEMORY_CENTER_GENERATOR_MAX_TOKENS=500

# LLM 冲突检测器配置
Environment=MEMORY_CENTER_DETECTOR_API_URL=https://api.siliconflow.cn/v1/chat/completions
Environment=MEMORY_CENTER_DETECTOR_API_KEY=__LLM_API_KEY__
Environment=MEMORY_CENTER_DETECTOR_MODEL=Qwen/Qwen2.5-7B-Instruct
Environment=MEMORY_CENTER_DETECTOR_TIMEOUT=30
Environment=MEMORY_CENTER_DETECTOR_MAX_TOKENS=500

# Embedding 语义检索配置（BAAI/bge-m3, 1024 维）
Environment=MEMORY_CENTER_EMBEDDER_API_URL=https://api.siliconflow.cn/v1/embeddings
Environment=MEMORY_CENTER_EMBEDDER_API_KEY=__LLM_API_KEY__
Environment=MEMORY_CENTER_EMBEDDER_MODEL=BAAI/bge-m3
Environment=MEMORY_CENTER_EMBEDDER_DIM=1024
Environment=MEMORY_CENTER_EMBEDDER_TIMEOUT=30

ExecStart=/opt/memory-center-demo/bin/memory-center-server
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
UNIT_EOF

# 用环境变量替换占位符（避免硬编码密钥）
sed -i "s|__LLM_API_KEY__|${LLM_API_KEY}|g" /etc/systemd/system/memory-center-demo.service
chmod 600 /etc/systemd/system/memory-center-demo.service
echo "systemd unit 已写入"

echo "=== [4/7] 写入 nginx 配置 ==="
cat > /etc/nginx/sites-enabled/memory-center-demo << 'NGINX_EOF'
server {
    listen 8088;
    server_name _;

    # 安全响应头
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-Frame-Options "DENY" always;
    # 请求体大小限制（archive 接口可能接收较大 turns 数组）
    client_max_body_size 10m;

    # REST API 反代（需鉴权）
    location / {
        proxy_pass http://127.0.0.1:8766;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 60s;
        proxy_send_timeout 60s;
    }

    # MCP Streamable HTTP 端点（SSE 流支持 + 鉴权校验）
    location /mcp {
        # 鉴权校验：要求 Authorization: Bearer <key>
        if ($http_authorization !~ "^Bearer .+$") {
            return 401 '{"error":{"code":"UNAUTHORIZED","message":"Missing or invalid Authorization header"}}';
        }
        proxy_pass http://127.0.0.1:8766;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header Connection "";
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
    }

    # 健康检查（无需鉴权）
    location /healthz {
        proxy_pass http://127.0.0.1:8766/healthz;
        proxy_set_header Host $host;
    }
}
NGINX_EOF
echo "nginx 配置已写入"

echo "=== [5/7] 启动服务 ==="
systemctl daemon-reload
systemctl enable memory-center-demo
systemctl start memory-center-demo
sleep 2
echo "服务状态："
systemctl status memory-center-demo --no-pager | head -8

echo "=== [6/7] 重载 nginx ==="
nginx -t && systemctl reload nginx
echo "nginx 已重载"

echo "=== [7/7] 验证 ==="
echo "--- 健康检查 ---"
curl -s http://127.0.0.1:8766/healthz || echo "FAIL: 8766 健康检查失败"
echo ""
echo "--- MCP 端点鉴权测试（无 Authorization 应返回 401）---"
curl -s -o /dev/null -w "HTTP %{http_code}" http://127.0.0.1:8088/mcp
echo " (期望 401)"
echo ""
echo "--- MCP 端点鉴权测试（带 Authorization 应非 401）---"
curl -s -o /dev/null -w "HTTP %{http_code}" -H "Authorization: Bearer trae-contest-demo-key-2026" http://127.0.0.1:8088/mcp
echo " (期望非 401)"
echo ""
echo "=== 部署完成 ==="
echo "Demo 访问地址："
echo "  REST API:  http://<your-server-ip>:8088/"
echo "  MCP 端点:  http://<your-server-ip>:8088/mcp"
echo "  健康检查:  http://<your-server-ip>:8088/healthz"
echo ""
echo "⚠️  安全提醒："
echo "  1. 请勿将 LLM_API_KEY 环境变量值提交到 Git"
echo "  2. 生产环境建议配置 HTTPS（Let's Encrypt + listen 443 ssl）"
