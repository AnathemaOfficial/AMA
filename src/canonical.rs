use crate::newtypes::*;

/// Type-safe canonical action. If this exists, it is structurally valid.
#[derive(Debug)]
pub enum CanonicalAction {
    FileWrite {
        path: WorkspacePath,
        content: BoundedBytes,
    },
    FileRead {
        path: WorkspacePath,
    },
    ShellExec {
        intent: IntentId,
        args: Vec<SafeArg>,
    },
    HttpRequest {
        method: HttpMethod,
        url: AllowlistedUrl,
        body: Option<BoundedBytes>,
    },
}

/// Typed result from actuation.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "type")]
pub enum ActionResult {
    #[serde(rename = "file_write")]
    FileWrite { bytes_written: u64 },
    #[serde(rename = "file_read")]
    FileRead {
        content: String,
        bytes_returned: u64,
        total_bytes: u64,
        truncated: bool,
    },
    #[serde(rename = "shell_exec")]
    ShellExec {
        stdout: String,
        stderr: String,
        exit_code: i32,
        truncated: bool,
    },
    #[serde(rename = "http_response")]
    HttpResponse {
        status_code: u16,
        body: String,
        truncated: bool,
    },
}
