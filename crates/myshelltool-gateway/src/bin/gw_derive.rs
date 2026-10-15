//! gw-derive —— 口令派生密钥的免交互工具（relay 容器 entrypoint 使用）。
//!
//! 用法：gw-derive <secret> <label> <pem|pub|pubhex>
//!   pem    — 输出私钥（OpenSSH PEM，relay 用它落 host key 文件）
//!   pub    — 输出公钥单行（authorized_keys 用）
//!   pubhex — 输出公钥 wire-format hex（pin 比对/展示用）

use myshelltool_gateway::keys::{derive_ed25519, public_key_hex, parse_public_key_line};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        eprintln!("用法: gw-derive <secret> <label> <pem|pub|pubhex>");
        std::process::exit(2);
    }
    match derive_ed25519(&args[0], &args[1]) {
        Ok((pem, pub_line)) => match args[2].as_str() {
            "pem" => print!("{pem}"),
            "pub" => println!("{pub_line}"),
            "pubhex" => {
                let pk = parse_public_key_line(&pub_line).expect("派生公钥解析");
                println!("{}", public_key_hex(&pk));
            }
            other => {
                eprintln!("未知输出形态: {other}");
                std::process::exit(2);
            }
        },
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
