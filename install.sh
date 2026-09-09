#!/bin/bash
set -e

echo "=== AgentBridge Linux Installer ==="

# 1. 检查 root 权限
if [ "$EUID" -ne 0 ]; then
    echo "请使用 sudo 运行此脚本: sudo bash install.sh"
    exit 1
fi

# 2. 获取最新版本
REPO="indexflowing/AgentBridge"
LATEST_TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
    echo "无法获取最新版本，尝试回退到主分支..."
    LATEST_TAG="v0.5.1"
fi

echo "正在下载 AgentBridge ${LATEST_TAG}..."
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/agentbridge-linux-amd64.tar.gz"

TMP_DIR=$(mktemp -d)
curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/agentbridge.tar.gz"
tar -xzf "$TMP_DIR/agentbridge.tar.gz" -C "$TMP_DIR"

# 3. 安装可执行文件及短别名
install -m 755 "$TMP_DIR/agentbridge" /usr/bin/agentbridge
ln -sf /usr/bin/agentbridge /usr/bin/agb

# 4. 初始化系统配置目录
mkdir -p /etc/agentbridge
if [ ! -f /etc/agentbridge/config.toml ]; then
    echo "生成默认配置文件: /etc/agentbridge/config.toml"
    /usr/bin/agentbridge init /var/www/agentbridge --port 8030 --local
    cp .agentbridge.toml /etc/agentbridge/config.toml || true
fi

# 5. 安装 systemd 服务
if [ -f "$TMP_DIR/agentbridge.service" ]; then
    cp "$TMP_DIR/agentbridge.service" /lib/systemd/system/agentbridge.service
    systemctl daemon-reload
    systemctl enable agentbridge
    systemctl restart agentbridge
    echo "AgentBridge systemd 服务已启动并设置开机自启！"
fi

rm -rf "$TMP_DIR"

echo "=== 安装完成 ==="
echo "  ➜ 主命令    : agentbridge"
echo "  ➜ 快捷别名  : agb"
echo "  ➜ 配置文件  : /etc/agentbridge/config.toml"
echo "  ➜ 服务管理  : sudo systemctl status agb"