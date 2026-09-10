#!/bin/bash
set -e

echo "=== AgentBridge Linux Installer ==="

# 1. 检查 root 权限
if [ "$EUID" -ne 0 ]; then
    echo "❌ 请使用 sudo 运行此脚本: sudo bash install.sh"
    exit 1
fi

# 2. 自动检测 CPU 硬件架构 (x86_64 vs aarch64)
ARCH=$(uname -m)
case "$ARCH" in
    x86_64|amd64)
        PKG_ARCH="amd64"
        ;;
    aarch64|arm64)
        PKG_ARCH="arm64"
        ;;
    *)
        echo "❌ 暂不支持的 CPU 架构: $ARCH"
        exit 1
        ;;
esac

echo "  ➜ 识别到系统架构: $PKG_ARCH"

# 3. 获取 GitHub Releases 最新版本号
REPO="IndexFlowing/AgentBridge"
LATEST_TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)

if [ -z "$LATEST_TAG" ]; then
    echo "  ➜ 无法从 GitHub API 获取最新版本，回退至默认版本 v1.0.1..."
    LATEST_TAG="v1.0.1"
fi

echo "  ➜ 正在下载 AgentBridge ${LATEST_TAG} (${PKG_ARCH})..."
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/agentbridge-linux-${PKG_ARCH}.tar.gz"

TMP_DIR=$(mktemp -d)
if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/agentbridge.tar.gz"; then
    echo "❌ 下载失败，请检查网络或确认该版本产物是否存在: $DOWNLOAD_URL"
    rm -rf "$TMP_DIR"
    exit 1
fi

tar -xzf "$TMP_DIR/agentbridge.tar.gz" -C "$TMP_DIR"

# 4. 安装可执行文件及短别名 (agb)
echo "  ➜ 安装二进制文件到 /usr/bin..."
install -m 755 "$TMP_DIR/agentbridge" /usr/bin/agentbridge
ln -sf /usr/bin/agentbridge /usr/bin/agb

# 5. 初始化系统配置目录 (/etc/agentbridge)
mkdir -p /etc/agentbridge
if [ ! -f /etc/agentbridge/config.toml ]; then
    echo "  ➜ 正在生成默认系统配置文件: /etc/agentbridge/config.toml..."
    mkdir -p /var/www/agentbridge
    /usr/bin/agentbridge init /var/www/agentbridge --port 8030 --local >/dev/null 2>&1 || true
    if [ -f .agentbridge.toml ]; then
        mv .agentbridge.toml /etc/agentbridge/config.toml
    fi
fi

# 6. 安装并启动 systemd 守护进程
if [ -f "$TMP_DIR/agentbridge.service" ]; then
    echo "  ➜ 配置 systemd 守护进程服务..."
    cp "$TMP_DIR/agentbridge.service" /lib/systemd/system/agentbridge.service
    systemctl daemon-reload
    systemctl enable agentbridge
    systemctl restart agentbridge
    echo "  ➜ 服务已启动并设置开机自启！"
fi

rm -rf "$TMP_DIR"

echo ""
echo "=== 🎉 AgentBridge 安装成功！==="
echo "  ➜ 主命令      : agentbridge"
echo "  ➜ 极速别名    : agb"
echo "  ➜ 配置文件    : /etc/agentbridge/config.toml"
echo "  ➜ 服务状态    : sudo systemctl status agb"
echo "  ➜ 实时日志    : sudo journalctl -u agb -f"
echo ""