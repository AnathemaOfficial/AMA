use sha2::{Sha256, Digest};

/// Audit log entry — metadata only, never contains payload content.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub timestamp: String,
    pub session_id: String,
    pub action_id: String,
    pub adapter: String,
    pub action: String,
    pub domain_id: String,
    pub magnitude_effective: u64,
    pub duration_ms: u64,
    pub status: String,      // "authorized" | "impossible" | "error"
    pub request_hash: String, // SHA-256 of canonical action
    pub truncated: bool,
}

/// Compute SHA-256 hash of the canonical action representation.
/// Hashes over (action, target, magnitude) — NOT raw JSON.
pub fn compute_request_hash(action: &str, target: &str, magnitude: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(action.as_bytes());
    hasher.update(b"|");
    hasher.update(target.as_bytes());
    hasher.update(b"|");
    hasher.update(magnitude.to_le_bytes());
    format!("{:x}", hasher.finalize())
}

/// Emit audit log entry via tracing (structured JSON).
pub fn log_audit(entry: &AuditEntry) {
    tracing::info!(
        timestamp = %entry.timestamp,
        session_id = %entry.session_id,
        action_id = %entry.action_id,
        adapter = %entry.adapter,
        action = %entry.action,
        domain_id = %entry.domain_id,
        magnitude = entry.magnitude_effective,
        duration_ms = entry.duration_ms,
        status = %entry.status,
        request_hash = %entry.request_hash,
        truncated = entry.truncated,
        "AMA_AUDIT"
    );
}
