use ama::config::*;
use tempfile::TempDir;
use std::fs;

#[test]
fn loads_valid_config() {
    let dir = TempDir::new().unwrap();
    let ws = dir.path().join("workspace");
    fs::create_dir(&ws).unwrap();

    write_test_configs(dir.path(), ws.to_str().unwrap());

    let result = AmaConfig::load(dir.path());
    assert!(result.is_ok());
}

#[test]
fn rejects_missing_config_file() {
    let dir = TempDir::new().unwrap();
    let result = AmaConfig::load(dir.path());
    assert!(result.is_err());
}

#[test]
fn rejects_relative_workspace_root() {
    let dir = TempDir::new().unwrap();
    write_test_configs(dir.path(), "./relative");
    let result = AmaConfig::load(dir.path());
    assert!(result.is_err());
}

#[test]
fn computes_sha256_hashes() {
    let dir = TempDir::new().unwrap();
    let ws = dir.path().join("workspace");
    fs::create_dir(&ws).unwrap();
    write_test_configs(dir.path(), ws.to_str().unwrap());

    let config = AmaConfig::load(dir.path()).unwrap();
    assert!(!config.boot_hashes.config_hash.is_empty());
    assert!(!config.boot_hashes.domains_hash.is_empty());
}

/// Helper: write minimal valid config files into a directory.
fn write_test_configs(dir: &std::path::Path, workspace_root: &str) {
    // On Windows, backslashes in TOML strings must be escaped.
    let workspace_root_escaped = workspace_root.replace('\\', "\\\\");
    fs::write(dir.join("config.toml"), format!(r#"
[ama]
workspace_root = "{workspace_root_escaped}"
bind_host = "127.0.0.1"
bind_port = 8787

[slime]
mode = "embedded"
max_capacity = 10000

[slime.domains.fs_write_workspace]
enabled = true
max_magnitude_per_action = 100

[slime.domains.fs_read_workspace]
enabled = true
max_magnitude_per_action = 500

[slime.domains.proc_exec_bounded]
enabled = true
max_magnitude_per_action = 50

[slime.domains.net_out_http]
enabled = true
max_magnitude_per_action = 200
"#)).unwrap();

    fs::write(dir.join("domains.toml"), r#"
[meta]
schema_version = "ama-domains-v1"
max_magnitude_claim = 1000

[domains.file_write]
domain_id = "fs.write.workspace"
max_payload_bytes = 1048576
validator = "relative_workspace_path"

[domains.file_read]
domain_id = "fs.read.workspace"
validator = "relative_workspace_path"

[domains.shell_exec]
domain_id = "proc.exec.bounded"
requires_intent = true

[domains.http_request]
domain_id = "net.out.http"
max_payload_bytes = 262144
validator = "allowlisted_url"
"#).unwrap();

    fs::write(dir.join("intents.toml"), r#"
[meta]
schema_version = "ama-intents-v1"
"#).unwrap();

    fs::write(dir.join("allowlist.toml"), r#"
[meta]
schema_version = "ama-allowlist-v1"
"#).unwrap();
}
