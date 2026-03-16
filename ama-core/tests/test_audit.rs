use ama_core::audit::*;

#[test]
fn request_hash_is_deterministic() {
    let hash1 = compute_request_hash("file_write", "test.txt", 1);
    let hash2 = compute_request_hash("file_write", "test.txt", 1);
    assert_eq!(hash1, hash2);
}

#[test]
fn request_hash_changes_with_input() {
    let hash1 = compute_request_hash("file_write", "test.txt", 1);
    let hash2 = compute_request_hash("file_write", "other.txt", 1);
    assert_ne!(hash1, hash2);
}

#[test]
fn request_hash_is_hex_sha256() {
    let hash = compute_request_hash("file_write", "test.txt", 1);
    assert_eq!(hash.len(), 64); // SHA-256 = 64 hex chars
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn audit_entry_formats_correctly() {
    let entry = AuditEntry {
        timestamp: "2026-03-13T12:00:00Z".into(),
        session_id: "test-session".into(),
        action_id: "test-action".into(),
        adapter: "generic".into(),
        action: "file_write".into(),
        domain_id: "fs.write.workspace".into(),
        magnitude_effective: 1,
        duration_ms: 42,
        status: "authorized".into(),
        request_hash: "abc123".into(),
        truncated: false,
    };
    assert_eq!(entry.action, "file_write");
    assert_eq!(entry.status, "authorized");
}
