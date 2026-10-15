# myshelltool-relay —— 网关模式的公网挂载点（VPS 侧）。
#
# 预配置 sshd 容器：任意一台有 Docker 的公网服务器 `docker run` 一条命令即成
# 挂载点，免改宿主 sshd_config。设计决策见 docs/计划/网关模式-实施计划.md。
#
# 两种凭据模式（二选一）：
# ① GATEWAY_SECRET（v0.21 推荐「服务器+口令」零文件口径）：host key 与
#    authorized_keys 全部从口令确定性派生（gw-derive.py，与跳板机/本机侧的
#    rust 实现 Argon2id 参数逐字节对齐）——跳板机只需输入「服务器地址+口令」。
# ② AUTHORIZED_KEYS（v0.20 bundle 文件口径）：传入挂载公钥单行；host key
#    volume 持久化（容器重建不换 key，网关 pin 不失配）。
#
# 关键配置（sshd_config）：GatewayPorts clientspecified / MaxSessions 100 /
# AllowTcpForwarding remote（最小权限）/ 禁密码 / 禁 pty / 纯 ed25519 host key。
#
# 运行（host network：remote-forward 挂载口是运行时动态监听，静态 -p 转不出）：
#   docker run -d --name myshelltool-relay --network host --restart=always \
#     -e GATEWAY_SECRET='<向导生成的口令>' \
#     ghcr.io/redtei/myshelltool-relay:latest
#
# 构建：docker build docker/relay -t myshelltool-relay（context 仅 docker/relay）

FROM alpine:3.20

# apk 走阿里镜像（大陆构建友好；海外亦可达）
RUN sed -i 's/dl-cdn.alpinelinux.org/mirrors.aliyun.com/g' /etc/apk/repositories \
    && apk add --no-cache openssh-server python3 py3-argon2-cffi py3-cryptography

# 挂载专用账户：无 shell（转发不需要；nologin 挡住交互登录路径）。
# shadow 置 '*'（无密码但**未锁定**）—— adduser -D 默认 '!' 锁定态，sshd 会以
# "account is locked" 拒绝公钥认证（2026-10-14 relay 冒烟实测抓出）。
RUN adduser -D -s /sbin/nologin -h /var/mount mount \
    && sed -i 's/^mount:!:/mount:*:/' /etc/shadow

COPY sshd_config /etc/ssh/sshd_config
COPY entrypoint.sh /entrypoint.sh
COPY gw-derive.py /usr/local/bin/gw-derive.py
# Windows checkout CRLF 防御：脚本在容器内按 LF 跑
RUN sed -i 's/\r$//' /entrypoint.sh /etc/ssh/sshd_config /usr/local/bin/gw-derive.py \
    && chmod +x /entrypoint.sh /usr/local/bin/gw-derive.py

# PORT：容器内 sshd 监听口（host network 下请用非 22 避开宿主 sshd，默认 22222）
# MOUNT_USER：挂载账户名（默认 mount；须与向导「挂载用户名」一致）
# GATEWAY_SECRET：口令派生模式（优先；与 AUTHORIZED_KEYS 二选一）
# AUTHORIZED_KEYS：bundle 文件模式的挂载公钥单行
ENV PORT=22222 \
    MOUNT_USER=mount \
    GATEWAY_SECRET="" \
    AUTHORIZED_KEYS=""

EXPOSE 22222

ENTRYPOINT ["/entrypoint.sh"]
