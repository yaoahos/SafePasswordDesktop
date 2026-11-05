//! 互操作验证：用 Rust 打开 Android core 生成的 .spv，输出明文 JSON。
//!
//! 用法: cargo run --example read_fixture -- <file.spv> <password-file> <out.json>
//! 密码从 UTF-8 文件读取（规避 shell 传参编码问题）。
use safepassword::sp::container::VaultContainer;
use std::fs;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: read_fixture <file.spv> <password-file> <out.json>");
        exit(2);
    }
    let data = fs::read(&args[1]).expect("read .spv");
    let password = fs::read_to_string(&args[2]).expect("read password file");
    let password = password.trim_end_matches(['\r', '\n']);
    match VaultContainer::open(&data, password) {
        Ok(container) => {
            let json = serde_json::to_string(&container.vault).expect("serialize");
            fs::write(&args[3], json).expect("write out.json");
            println!(
                "OK rust-open items={} name={}",
                container.vault.items.len(),
                container.vault.name
            );
        }
        Err(e) => {
            eprintln!("open failed: {e}");
            exit(1);
        }
    }
}
