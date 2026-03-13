use crate::errors::AmaError;
use std::path::{Path, PathBuf};

/// A path guaranteed to be inside workspace_root, with no traversal or symlinks.
#[derive(Debug, Clone)]
pub struct WorkspacePath {
    canonical: PathBuf,
    relative: String,
}

impl WorkspacePath {
    pub fn new(relative: &str, workspace_root: &Path) -> Result<Self, AmaError> {
        if relative.is_empty() {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "empty path".into(),
            });
        }
        if relative.starts_with('/') || relative.starts_with('\\') || relative.contains(':') {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "absolute paths forbidden".into(),
            });
        }
        for segment in relative.split(['/', '\\']) {
            if segment == ".." {
                return Err(AmaError::Validation {
                    error_class: "invalid_target".into(),
                    message: "path traversal forbidden".into(),
                });
            }
            if segment.is_empty() && relative.contains("//") {
                return Err(AmaError::Validation {
                    error_class: "invalid_target".into(),
                    message: "ambiguous path segment".into(),
                });
            }
        }
        let joined = workspace_root.join(relative);
        let canonical = joined;
        Ok(Self {
            canonical,
            relative: relative.to_string(),
        })
    }

    pub fn canonical(&self) -> &Path { &self.canonical }
    pub fn relative(&self) -> &str { &self.relative }
}

/// Bytes guaranteed to be valid UTF-8 and within size limit.
#[derive(Debug, Clone)]
pub struct BoundedBytes(String);

impl BoundedBytes {
    pub fn new(data: String, max_bytes: usize) -> Result<Self, AmaError> {
        if data.len() > max_bytes {
            return Err(AmaError::Validation {
                error_class: "payload_too_large".into(),
                message: format!("payload {} bytes exceeds limit {}", data.len(), max_bytes),
            });
        }
        Ok(Self(data))
    }

    pub fn as_str(&self) -> &str { &self.0 }
    pub fn len(&self) -> usize { self.0.len() }
    pub fn is_empty(&self) -> bool { self.0.is_empty() }
}

/// A shell argument guaranteed to have no null bytes and be non-empty.
#[derive(Debug, Clone)]
pub struct SafeArg(String);

impl SafeArg {
    pub fn new(arg: &str) -> Result<Self, AmaError> {
        if arg.is_empty() {
            return Err(AmaError::Validation {
                error_class: "invalid_args".into(),
                message: "empty argument".into(),
            });
        }
        if arg.contains('\0') {
            return Err(AmaError::Validation {
                error_class: "invalid_args".into(),
                message: "null byte in argument".into(),
            });
        }
        Ok(Self(arg.to_string()))
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

/// An intent ID that exists in intents.toml. Alphanumeric + underscore only.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IntentId(String);

impl IntentId {
    pub fn new(id: &str) -> Result<Self, AmaError> {
        if id.is_empty() {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "empty intent id".into(),
            });
        }
        if !id.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "intent id must be alphanumeric/underscore".into(),
            });
        }
        Ok(Self(id.to_string()))
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

/// Allowlist entry for URL matching.
#[derive(Debug, Clone)]
pub struct AllowlistEntry {
    pub pattern: String,
    pub methods: Vec<String>,
    pub max_body_bytes: Option<usize>,
}

/// A URL guaranteed to be HTTPS and matched against the allowlist.
#[derive(Debug, Clone)]
pub struct AllowlistedUrl {
    url: String,
}

impl AllowlistedUrl {
    pub fn new(url: &str, allowlist: &[AllowlistEntry]) -> Result<Self, AmaError> {
        if !url.starts_with("https://") {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "only https URLs allowed".into(),
            });
        }
        if let Some(authority) = url.strip_prefix("https://") {
            if authority.contains('@') {
                return Err(AmaError::Validation {
                    error_class: "invalid_target".into(),
                    message: "userinfo in URL forbidden".into(),
                });
            }
        }
        if url.contains('#') {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "fragments in URL forbidden".into(),
            });
        }
        let matched = allowlist.iter().any(|entry| {
            glob_match(&entry.pattern, url)
        });
        if !matched {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "URL not in allowlist".into(),
            });
        }
        Ok(Self { url: url.to_string() })
    }

    pub fn as_str(&self) -> &str { &self.url }
}

/// Simple glob matching: `*` matches any suffix.
fn glob_match(pattern: &str, url: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        url.starts_with(prefix)
    } else {
        url == pattern
    }
}

/// HTTP method — GET or POST only in P0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    pub fn parse(s: &str) -> Result<Self, AmaError> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            _ => Err(AmaError::Validation {
                error_class: "invalid_method".into(),
                message: format!("unsupported method: {}", s),
            }),
        }
    }
}
