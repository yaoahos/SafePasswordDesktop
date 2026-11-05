//! 互操作反向验证：Rust 重新加密保险库，供 Android core 打开。
//!
//! 用法: cargo run --example rewrite_fixture -- <in.spv> <password-file> <out.spv>
use safepassword::sp::container::VaultContainer;
use std::fs;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: rewrite_fixture <in.spv> <password-file> <out.spv>");
        exit(2);
    }
    let data = fs::read(&args[1]).expect("read in.spv");
    let password = fs::read_to_string(&args[2]).expect("read password file");
    let password = password.trim_end_matches(['\r', '\n']);
    let container = match VaultContainer::open(&data, password) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("open failed: {e}");
            exit(1);
        }
    };
    let mut rewrapped = VaultContainer::create(container.vault.clone(), password);
    fs::write(&args[3], rewrapped.to_bytes()).expect("write out.spv");
    println!("OK rust-rewrite bytes={}", args[3]);
}
