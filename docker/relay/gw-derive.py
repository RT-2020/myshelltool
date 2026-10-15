#!/usr/bin/env python3
"""gw-derive —— 口令派生密钥（relay 容器用，python 版）。

与 crates/myshelltool-gateway keys.rs 的 derive_ed25519 互通：
Argon2id(secret, "myshelltool-gateway:<label>")，参数对齐 rust argon2 0.5 默认
（m=19456 KiB / t=2 / p=1 / v19）→ 32 字节 ed25519 seed → 私钥/公钥。

用法: gw-derive.py <secret> <label> <pem|pub|pubhex>
  pem    — OpenSSH 私钥（sshd HostKey 文件）
  pub    — authorized_keys 单行
  pubhex — 公钥 wire-format hex（pin 比对）
"""
import base64
import struct
import sys

import argon2
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


def ssh_string(b: bytes) -> bytes:
    return struct.pack(">I", len(b)) + b


def derive_seed(secret: str, label: str) -> bytes:
    salt = f"myshelltool-gateway:{label}".encode()
    return argon2.low_level.hash_secret_raw(
        secret=secret.encode(), salt=salt,
        time_cost=2, memory_cost=19456, parallelism=1,
        hash_len=32, type=argon2.low_level.Type.ID, version=19,
    )


def main():
    if len(sys.argv) != 4:
        sys.exit("用法: gw-derive.py <secret> <label> <pem|pub|pubhex>")
    secret, label, mode = sys.argv[1], sys.argv[2], sys.argv[3]
    seed = derive_seed(secret, label)
    priv = Ed25519PrivateKey.from_private_bytes(seed)
    pub = priv.public_key().public_bytes_raw()
    # 公钥 wire format：string("ssh-ed25519") + string(pub)——两层都要长度前缀
    blob = ssh_string(b"ssh-ed25519") + ssh_string(pub)

    if mode == "pubhex":
        print(blob.hex())
        return
    if mode == "pub":
        print("ssh-ed25519 " + base64.b64encode(blob).decode())
        return
    if mode != "pem":
        sys.exit(f"未知输出形态: {mode}")

    # OpenSSH 私钥格式（无加密 cipher=none；draft-miller-ssh-agent §3）：
    # magic "openssh-key-v1\0" + cipher/kdf/nkeys + pub blob + 私钥段（含 8 字节
    # 对齐 padding——未加密也要求）。sshd 只认此格式。
    ck = 0x5F3759DF  # checkint（同一值重复两次；自检用，任意值）
    # ed25501 私钥段（draft §7）：string(keytype) + string(pk 32B) + string(sk 64B=seed||pub)
    # + string(comment)——pk 与 sk 是两个字段（与 ssh_key crate 输出逐字节对齐）
    inner = (struct.pack(">II", ck, ck) + ssh_string(b"ssh-ed25519")
             + ssh_string(pub) + ssh_string(seed + pub) + ssh_string(b""))
    pad = bytes(range(1, (-len(inner)) % 8 + 1))
    inner += pad
    body = (b"openssh-key-v1\x00"
            + ssh_string(b"none") + ssh_string(b"none") + ssh_string(b"")
            + struct.pack(">I", 1)
            + ssh_string(blob)
            + ssh_string(inner))
    b64 = base64.b64encode(body).decode()
    lines = [b64[i:i + 70] for i in range(0, len(b64), 70)]
    # 二进制写出（\n 换行）——print 在 Windows 重定向下产 CRLF，\r 混入 base64
    # 会使 openssh 报 libcrypto 错误
    out = sys.stdout.buffer
    out.write(b"-----BEGIN OPENSSH PRIVATE KEY-----\n")
    out.write(("\n".join(lines) + "\n").encode())
    out.write(b"-----END OPENSSH PRIVATE KEY-----\n")
    out.flush()


if __name__ == "__main__":
    main()
