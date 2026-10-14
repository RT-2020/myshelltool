# myshelltool-relay —— 网关模式的公网挂载点（VPS 侧）。
#
# 预配置 sshd 容器：任意一台有 Docker 的公网服务器 `docker run` 一条命令即成
# 挂载点，免改宿主 sshd_config。设计决策见 docs/计划/网关模式-实施计划.md §5 决策 17。
#
# 关键配置（sshd_config）：
# - GatewayPorts clientspecified —— 挂载口可绑 0.0.0.0（本机直连必需）
# - MaxSessions 100 —— 经网关的每条连接占挂载连接一个 channel（默认 10 易撞限）
# - AllowTcpForwarding remote —— 挂载器只需 remote forward，最小权限
# - 禁密码 / 禁 pty / 禁 agent / 禁 X11 / root 禁登 —— 无面攻击面
#
# 运行（host network：remote-forward 挂载口是运行时动态监听，静态 -p 转不出）：
#   docker run -d --name myshelltool-relay --network host --restart=always \
#     -v /data/myshelltool-relay:/etc/ssh/hostkeys \
#     -e AUTHORIZED_KEYS='ssh-ed25519 AAAA... （向导第一步生成的挂载公钥）' \
#     ghcr.io/redtei/myshelltool-relay:latest
#
# host key 首启生成并持久化到 volume（容器重建不换 key，否则网关 pin 失配）。

FROM alpine:3.20

RUN apk add --no-cache openssh-server

# 挂载专用账户：无 shell（转发不需要；nologin 同时挡住任何交互登录路径）。
# shadow 置 '*'（无密码但**未锁定**）—— adduser -D 默认 '!' 锁定态，sshd 会以
# "account is locked" 拒绝公钥认证（2026-10-14 relay 冒烟实测抓出）。
RUN adduser -D -s /sbin/nologin -h /var/mount mount \
    && sed -i 's/^mount:!:/mount:*:/' /etc/shadow

COPY sshd_config /etc/ssh/sshd_config
COPY entrypoint.sh /entrypoint.sh
# Windows checkout CRLF 防御：脚本在容器内按 LF 跑
RUN sed -i 's/\r$//' /entrypoint.sh /etc/ssh/sshd_config && chmod +x /entrypoint.sh

# PORT：容器内 sshd 监听口（host network 下请用非 22 避开宿主 sshd，默认 22222）
# MOUNT_USER：挂载账户名（默认 mount；须与向导「挂载用户名」一致）
# AUTHORIZED_KEYS：向导第一步生成的挂载公钥单行（authorized_keys 格式）
ENV PORT=22222 \
    MOUNT_USER=mount \
    AUTHORIZED_KEYS=""

EXPOSE 22222

ENTRYPOINT ["/entrypoint.sh"]
