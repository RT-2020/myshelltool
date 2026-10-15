#!/bin/sh
# myshelltool-relay 入口：凭据模式分流（GATEWAY_SECRET 派生 / AUTHORIZED_KEYS 文件）
# + host key 就位 + 前台 sshd。
set -eu

HOSTKEY_DIR=/etc/ssh/hostkeys
AUTH_KEYS=/etc/ssh/authorized_keys
MOUNT_USER="${MOUNT_USER:-mount}"

mkdir -p "$HOSTKEY_DIR"

if [ -n "${GATEWAY_SECRET}" ]; then
    # ─── 口令派生模式（v0.21 零文件口径）───
    # host key 与挂载公钥都从口令确定性派生：同口令永远同身份，无需 volume
    # 持久化（容器重建/换机重部署，身份不变，跳板机与本机侧零感知）。
    echo "[relay] GATEWAY_SECRET mode: deriving host key and authorized_keys from secret"
    gw-derive.py "$GATEWAY_SECRET" relay-hostkey pem > "$HOSTKEY_DIR/ssh_host_ed25519_key"
    chmod 600 "$HOSTKEY_DIR/ssh_host_ed25519_key"
    printf '%s\n' "$(gw-derive.py "$GATEWAY_SECRET" mount pub)" > "$AUTH_KEYS"
else
    # ─── bundle 文件模式（v0.20）：公钥经 env 注入，host key 持久化 volume ───
    if [ ! -f "$HOSTKEY_DIR/ssh_host_ed25519_key" ]; then
        echo "[relay] generating persistent host keys in $HOSTKEY_DIR"
        ssh-keygen -t ed25519 -f "$HOSTKEY_DIR/ssh_host_ed25519_key" -N '' -q
    fi
    if [ -z "${AUTHORIZED_KEYS}" ]; then
        echo "[relay] FATAL: 需配置 GATEWAY_SECRET（口令模式）或 AUTHORIZED_KEYS（bundle 模式）其一" >&2
        exit 1
    fi
    printf '%s\n' "$AUTHORIZED_KEYS" > "$AUTH_KEYS"
fi

# authorized_keys owner 必须是目标挂载用户（sshd 认证阶段以其身份读取）
chown "${MOUNT_USER}:${MOUNT_USER}" "$AUTH_KEYS"
chmod 600 "$AUTH_KEYS"

# 监听口（host network 下避开宿主 22）
sed -i "s/^Port .*/Port ${PORT:-22222}/" /etc/ssh/sshd_config

echo "[relay] sshd listening on port ${PORT:-22222} (host network: mount port binds on the host directly)"
exec /usr/sbin/sshd -D -e
