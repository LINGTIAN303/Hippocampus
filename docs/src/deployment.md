# 部署指南

> 本章节是 [GitHub Wiki: Deployment](https://github.com/LINGTIAN303/MemoryCenter/wiki/Deployment) 的镜像。

## 部署方式选择

| 方式 | 适用场景 | 复杂度 |
|------|---------|--------|
| 单二进制 + systemd | 单机生产部署 | ⭐ |
| Docker | 容器化环境 | ⭐⭐ |
| Nginx 反代 + systemd | 需要 HTTPS / 多服务统一入口 | ⭐⭐ |
| Cloudflare CDN + Nginx | 全球加速 + DDoS 防护 | ⭐⭐⭐ |

## 单二进制 + systemd（最简）

### 1. 构建二进制

```bash
cargo build --release -p memory-center-server
# 产物：target/release/memory-center-server
```

### 2. systemd 配置

```ini
# /etc/systemd/system/memory-center.service
[Unit]
Description=MemoryCenter Server
After=network.target

[Service]
Type=simple
User=mc
WorkingDirectory=/opt/memorycenter
Environment=MEMORY_CENTER_HOST=127.0.0.1
Environment=MEMORY_CENTER_PORT=8765
Environment=MEMORY_CENTER_ROOT=/var/lib/memorycenter/data
Environment=MEMORY_CENTER_MCP_ENABLED=true
Environment=MEMORY_CENTER_MCP_ALLOWED_HOSTS=your-domain.com,localhost,127.0.0.1
ExecStart=/opt/memorycenter/memory-center-server
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

### 3. 启动

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now memory-center
sudo systemctl status memory-center
```

## Nginx 反代配置

```nginx
server {
    listen 443 ssl http2;
    server_name your-domain.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location /memory-center/ {
        proxy_pass http://127.0.0.1:8765/;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;  # SSE 流需要
    }
}
```

## 完整文档

完整部署文档（含 Docker / Cloudflare / 备份 / 监控 / 排查）见：
- [Wiki: Deployment](https://github.com/LINGTIAN303/MemoryCenter/wiki/Deployment)
- [deploy/ 目录](https://github.com/LINGTIAN303/MemoryCenter/tree/main/deploy)
