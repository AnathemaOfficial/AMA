use safa_core::newtypes::*;
use std::path::PathBuf;

#[test]
fn workspace_path_rejects_traversal() {
    let root = PathBuf::from("/tmp/safa-workspace");
    assert!(WorkspacePath::new("../../etc/passwd", &root).is_err());
    assert!(WorkspacePath::new("../outside", &root).is_err());
    assert!(WorkspacePath::new("foo/../../bar", &root).is_err());
}

#[test]
fn workspace_path_rejects_absolute() {
    let root = PathBuf::from("/tmp/safa-workspace");
    assert!(WorkspacePath::new("/etc/passwd", &root).is_err());
}

#[test]
fn workspace_path_accepts_valid() {
    let root = PathBuf::from("/tmp/safa-workspace");
    assert!(WorkspacePath::new("", &root).is_err()); // empty
}

#[test]
fn bounded_bytes_rejects_oversized() {
    let data = "x".repeat(1_048_577); // 1 MiB + 1
    assert!(BoundedBytes::new(data, 1_048_576).is_err());
}

#[test]
fn bounded_bytes_accepts_within_limit() {
    let data = "hello world".to_string();
    assert!(BoundedBytes::new(data, 1_048_576).is_ok());
}

#[test]
fn safe_arg_rejects_null_bytes() {
    assert!(SafeArg::new("hello\0world").is_err());
}

#[test]
fn safe_arg_rejects_empty() {
    assert!(SafeArg::new("").is_err());
}

#[test]
fn safe_arg_accepts_valid() {
    assert!(SafeArg::new("src/main.rs").is_ok());
}

#[test]
fn intent_id_rejects_traversal() {
    assert!(IntentId::new("../bad").is_err());
    assert!(IntentId::new("good_intent").is_ok());
}

#[test]
fn allowlisted_url_rejects_http() {
    assert!(AllowlistedUrl::new("http://example.com", &[]).is_err());
}

#[test]
fn allowlisted_url_rejects_not_in_list() {
    let patterns = vec![AllowlistEntry {
        pattern: "https://api.github.com/*".to_string(),
        methods: vec!["GET".to_string()],
        max_body_bytes: None,
    }];
    assert!(AllowlistedUrl::new("https://evil.com/steal", &patterns).is_err());
}
