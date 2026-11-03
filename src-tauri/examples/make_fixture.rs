//! 由明文 JSON 重新生成互操作夹具（容器格式变更后重建 .spv）。
//!
//! 用法: cargo run --example make_fixture -- <vault.json> <password-file> <out.spv>
use safepassword::sp::container::VaultContainer;
use safepassword::sp::model::Vault;
use std::fs;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: make_fixture <vault.json> <password-file> <out.spv>");
        exit(2);
    }
    let json = fs::read_to_string(&args[1]).expect("read vault.json");
    let vault: Vault = serde_json::from_str(&json).expect("parse vault");
    let password = fs::read_to_string(&args[2]).expect("read password file");
    let password = password.trim_end_matches(['\r', '\n']);
    let mut container = VaultContainer::create(vault, password);
    let bytes = container.to_bytes();
    fs::write(&args[3], &bytes).expect("write out.spv");
    println!(
        "OK items={} name={} bytes={} -> {}",
        container.vault.items.len(),
        container.vault.name,
        bytes.len(),
        args[3]
    );
}
