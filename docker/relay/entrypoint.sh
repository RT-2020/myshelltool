#!/bin/sh
# myshelltool-relay 入口：host key 持久化 + authorized_keys 注入 + 前台 sshd。
set -eu

HOSTKEY_DIR=/etc/ssh/hostkeys
AUTH_KEYS=/etc/ssh/authorized_keys

mkdir -p "$HOSTKEY_DIR"

# host key：volume 无则生成（容器重建复用同一批 key——网关 pin 不失配）
if [ ! -f "$HOSTKEY_DIR/ssh_host_ed25519_key" ]; then
    echo "[relay] generating persistent host keys in $HOSTKEY_DIR"
    ssh-keygen -t ed25519 -f "$HOSTKEY_DIR/ssh_host_ed25519_key" -N '' -q
    ssh-keygen -t rsa -b 3072 -f "$HOSTKEY_DIR/ssh_host_rsa_key" -N '' -q
fi

# authorized_keys：必填（禁密码认证下空 = 谁也连不上，fail-fast 报错退出）
if [ -z "${AUTHORIZED_KEYS}" ]; then
    echo "[relay] FATAL: AUTHORIZED_KEYS env is empty — pass the mount public key from the wizard (docker run -e AUTHORIZED_KEYS='ssh-ed25519 AAAA...')" >&2
    exit 1
fi
printf '%s\n' "$AUTHORIZED_KEYS" > "$AUTH_KEYS"
# owner 必须是目标挂载用户（sshd 认证阶段以其身份读取；root:root 600 会
# Permission denied——2026-10-14 relay 冒烟实测抓出）
chown "${MOUNT_USER:-mount}:${MOUNT_USER:-mount}" "$AUTH_KEYS"
chmod 600 "$AUTH_KEYS"

# 监听口（host network 下避开宿主 22）与挂载账户名可覆盖
sed -i "s/^Port .*/Port ${PORT:-22222}/" /etc/ssh/sshd_config

echo "[relay] sshd listening on port ${PORT:-22222} (host network: mount port binds on the host directly)"
exec /usr/sbin/sshd -D -e
