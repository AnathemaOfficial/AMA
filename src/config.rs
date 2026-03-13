use crate::errors::AmaError;
use crate::newtypes::AllowlistEntry;
use sha2::{Sha256, Digest};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ── Boot Hashes ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BootHashes {
    pub config_hash: String,
    pub domains_hash: String,
    pub intents_hash: String,
    pub allowlist_hash: String,
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

// ── TOML raw structs (serde) ─────────────────────────────────

#[derive(Deserialize)]
struct RawConfig {
    ama: RawAma,
    slime: RawSlime,
}

#[derive(Deserialize)]
struct RawAma {
    workspace_root: String,
    #[serde(default = "default_host")]
    bind_host: String,
    #[serde(default = "default_port")]
    bind_port: u16,
    #[serde(default = "default_log_level")]
    log_level: String,
    #[serde(default = "default_log_output")]
    log_output: String,
}

fn default_host() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 8787 }
fn default_log_level() -> String { "info".into() }
fn default_log_output() -> String { "stderr".into() }

#[derive(Deserialize)]
struct RawSlime {
    mode: String,
    max_capacity: u64,
    domains: HashMap<String, RawDomainPolicy>,
}

#[derive(Deserialize, Clone)]
struct RawDomainPolicy {
    enabled: bool,
    max_magnitude_per_action: u64,
}

#[derive(Deserialize)]
struct RawDomains {
    meta: RawMeta,
    domains: HashMap<String, RawDomainEntry>,
}

#[derive(Deserialize)]
struct RawMeta {
    schema_version: String,
}

#[derive(Deserialize, Clone)]
struct RawDomainEntry {
    domain_id: String,
    #[serde(default)]
    max_payload_bytes: Option<usize>,
    #[serde(default)]
    validator: Option<String>,
    #[serde(default)]
    requires_intent: Option<bool>,
}

#[derive(Deserialize)]
struct RawIntents {
    meta: RawMeta,
    #[serde(default)]
    intents: HashMap<String, RawIntentEntry>,
}

#[derive(Deserialize, Clone)]
struct RawIntentEntry {
    binary: String,
    args_template: Vec<String>,
    #[serde(default)]
    validators: Vec<String>,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
struct RawAllowlist {
    meta: RawMeta,
    #[serde(default)]
    urls: Vec<RawAllowlistUrl>,
}

#[derive(Deserialize, Clone)]
struct RawAllowlistUrl {
    pattern: String,
    methods: Vec<String>,
    #[serde(default)]
    max_body_bytes: Option<usize>,
}

// ── Public types ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DomainPolicy {
    pub enabled: bool,
    pub max_magnitude_per_action: u64,
}

#[derive(Debug, Clone)]
pub struct DomainMapping {
    pub domain_id: String,
    pub max_payload_bytes: Option<usize>,
    pub validator: Option<String>,
    pub requires_intent: bool,
}

#[derive(Debug, Clone)]
pub struct IntentMapping {
    pub binary: String,
    pub args_template: Vec<String>,
    pub validators: Vec<String>,
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AmaConfig {
    pub workspace_root: PathBuf,
    pub bind_host: String,
    pub bind_port: u16,
    pub log_level: String,
    pub log_output: String,
    pub slime_mode: String,
    pub max_capacity: u64,
    pub domain_policies: HashMap<String, DomainPolicy>,
    pub domain_mappings: HashMap<String, DomainMapping>,
    pub intents: HashMap<String, IntentMapping>,
    pub allowlist: Vec<AllowlistEntry>,
    pub boot_hashes: BootHashes,
}

impl AmaConfig {
    /// Load and validate all config files. Refuses to return on any error.
    pub fn load(config_dir: &Path) -> Result<Self, AmaError> {
        // ── Read raw files ───────────────────────────────────
        let config_bytes = Self::read_file(config_dir, "config.toml")?;
        let domains_bytes = Self::read_file(config_dir, "domains.toml")?;
        let intents_bytes = Self::read_file(config_dir, "intents.toml")?;
        let allowlist_bytes = Self::read_file(config_dir, "allowlist.toml")?;

        // ── Compute SHA-256 hashes ───────────────────────────
        let boot_hashes = BootHashes {
            config_hash: sha256_hex(&config_bytes),
            domains_hash: sha256_hex(&domains_bytes),
            intents_hash: sha256_hex(&intents_bytes),
            allowlist_hash: sha256_hex(&allowlist_bytes),
        };

        // ── Parse TOML ───────────────────────────────────────
        let raw_config: RawConfig = toml::from_str(
            std::str::from_utf8(&config_bytes).map_err(|e| Self::boot_err(format!("config.toml not UTF-8: {e}")))?
        ).map_err(|e| Self::boot_err(format!("config.toml parse error: {e}")))?;

        let raw_domains: RawDomains = toml::from_str(
            std::str::from_utf8(&domains_bytes).map_err(|e| Self::boot_err(format!("domains.toml not UTF-8: {e}")))?
        ).map_err(|e| Self::boot_err(format!("domains.toml parse error: {e}")))?;

        let raw_intents: RawIntents = toml::from_str(
            std::str::from_utf8(&intents_bytes).map_err(|e| Self::boot_err(format!("intents.toml not UTF-8: {e}")))?
        ).map_err(|e| Self::boot_err(format!("intents.toml parse error: {e}")))?;

        let raw_allowlist: RawAllowlist = toml::from_str(
            std::str::from_utf8(&allowlist_bytes).map_err(|e| Self::boot_err(format!("allowlist.toml not UTF-8: {e}")))?
        ).map_err(|e| Self::boot_err(format!("allowlist.toml parse error: {e}")))?;

        // ── Validate schema versions ─────────────────────────
        Self::check_schema("ama-domains-v1", &raw_domains.meta.schema_version, "domains.toml")?;
        Self::check_schema("ama-intents-v1", &raw_intents.meta.schema_version, "intents.toml")?;
        Self::check_schema("ama-allowlist-v1", &raw_allowlist.meta.schema_version, "allowlist.toml")?;

        // ── Validate workspace_root ──────────────────────────
        let workspace_root = PathBuf::from(&raw_config.ama.workspace_root);
        if !workspace_root.is_absolute() {
            return Err(Self::boot_err("workspace_root must be absolute".into()));
        }
        if !workspace_root.is_dir() {
            return Err(Self::boot_err(format!(
                "workspace_root does not exist or is not a directory: {}",
                workspace_root.display()
            )));
        }

        // ── Validate bind_host ───────────────────────────────
        if raw_config.ama.bind_host != "127.0.0.1" {
            return Err(Self::boot_err(format!(
                "P0 requires bind_host = 127.0.0.1, got '{}'",
                raw_config.ama.bind_host
            )));
        }

        // ── Validate slime mode ──────────────────────────────
        if raw_config.slime.mode != "embedded" {
            return Err(Self::boot_err(format!(
                "P0 requires slime.mode = embedded, got '{}'",
                raw_config.slime.mode
            )));
        }
        if raw_config.slime.max_capacity == 0 {
            return Err(Self::boot_err("slime.max_capacity must be > 0".into()));
        }

        // ── Build domain policies (underscore -> dot normalization) ──
        let mut domain_policies = HashMap::new();
        for (key, raw_policy) in &raw_config.slime.domains {
            let domain_id = key.replace('_', ".");
            if raw_policy.max_magnitude_per_action == 0 {
                return Err(Self::boot_err(format!(
                    "domain '{}': max_magnitude_per_action must be > 0", domain_id
                )));
            }
            if raw_policy.max_magnitude_per_action > raw_config.slime.max_capacity {
                return Err(Self::boot_err(format!(
                    "domain '{}': max_magnitude_per_action ({}) > max_capacity ({})",
                    domain_id, raw_policy.max_magnitude_per_action, raw_config.slime.max_capacity
                )));
            }
            domain_policies.insert(domain_id, DomainPolicy {
                enabled: raw_policy.enabled,
                max_magnitude_per_action: raw_policy.max_magnitude_per_action,
            });
        }

        // ── Build domain mappings ────────────────────────────
        let mut domain_mappings = HashMap::new();
        for (action, entry) in &raw_domains.domains {
            // Cross-reference: domain_id must exist in config.toml policies
            if !domain_policies.contains_key(&entry.domain_id) {
                return Err(Self::boot_err(format!(
                    "domains.toml action '{}' references domain_id '{}' not in config.toml",
                    action, entry.domain_id
                )));
            }
            domain_mappings.insert(action.clone(), DomainMapping {
                domain_id: entry.domain_id.clone(),
                max_payload_bytes: entry.max_payload_bytes,
                validator: entry.validator.clone(),
                requires_intent: entry.requires_intent.unwrap_or(false),
            });
        }

        // ── Build intent mappings ────────────────────────────
        let mut intents = HashMap::new();
        for (name, raw_intent) in &raw_intents.intents {
            // On Linux, verify binary exists (skip on Windows for dev)
            #[cfg(unix)]
            {
                let bin_path = Path::new(&raw_intent.binary);
                if !bin_path.exists() {
                    return Err(Self::boot_err(format!(
                        "intent '{}': binary '{}' does not exist", name, raw_intent.binary
                    )));
                }
            }
            let working_dir = raw_intent.working_dir.as_ref().map(|wd| {
                wd.replace("{{workspace_root}}", workspace_root.to_str().unwrap_or(""))
            });
            intents.insert(name.clone(), IntentMapping {
                binary: raw_intent.binary.clone(),
                args_template: raw_intent.args_template.clone(),
                validators: raw_intent.validators.clone(),
                working_dir,
            });
        }

        // ── Build allowlist ──────────────────────────────────
        let allowlist: Vec<AllowlistEntry> = raw_allowlist.urls.iter().map(|u| {
            AllowlistEntry {
                pattern: u.pattern.clone(),
                methods: u.methods.clone(),
                max_body_bytes: u.max_body_bytes,
            }
        }).collect();

        Ok(Self {
            workspace_root,
            bind_host: raw_config.ama.bind_host,
            bind_port: raw_config.ama.bind_port,
            log_level: raw_config.ama.log_level,
            log_output: raw_config.ama.log_output,
            slime_mode: raw_config.slime.mode,
            max_capacity: raw_config.slime.max_capacity,
            domain_policies,
            domain_mappings,
            intents,
            allowlist,
            boot_hashes,
        })
    }

    fn read_file(dir: &Path, name: &str) -> Result<Vec<u8>, AmaError> {
        let path = dir.join(name);
        fs::read(&path).map_err(|e| Self::boot_err(format!(
            "cannot read {}: {}", path.display(), e
        )))
    }

    fn check_schema(expected: &str, got: &str, file: &str) -> Result<(), AmaError> {
        if got != expected {
            return Err(Self::boot_err(format!(
                "{}: unrecognized schema_version '{}' (expected '{}')",
                file, got, expected
            )));
        }
        Ok(())
    }

    fn boot_err(msg: String) -> AmaError {
        AmaError::ServiceUnavailable { message: msg }
    }
}
