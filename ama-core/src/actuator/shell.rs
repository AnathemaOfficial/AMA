use crate::errors::AmaError;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Result of a shell exec operation.
#[derive(Debug)]
pub struct ShellExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub truncated: bool,
}

/// Execute a binary with arguments in a new process group.
///
/// - Uses execv (via Command::new), never sh -c
/// - Fresh minimal environment
/// - Process group isolation (setpgid)
/// - Kill sequence: SIGTERM -> 2s -> SIGKILL to PGID
/// - Bounded output capture
pub async fn shell_exec(
    binary: &str,
    args: &[&str],
    working_dir: &str,
    action_id: &str,
    timeout: Duration,
    max_output_bytes: usize,
) -> Result<ShellExecResult, AmaError> {
    use std::os::unix::process::CommandExt;

    let mut cmd = Command::new(binary);
    cmd.args(args)
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Fresh minimal environment — no inherited variables
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", working_dir)
        .env("LANG", "en_US.UTF-8")
        .env("AMA_ACTION_ID", action_id);

    // SAFETY: pre_exec runs after fork, before exec in child process.
    // setpgid(0,0) puts child in its own process group for kill containment.
    unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }

    let mut child = cmd.spawn().map_err(|e| AmaError::ServiceUnavailable {
        message: format!("failed to spawn process: {}", e),
    })?;

    let pid = child.id().unwrap_or(0) as i32;

    // Take ownership of stdout/stderr handles
    let mut stdout_handle = child.stdout.take().unwrap();
    let mut stderr_handle = child.stderr.take().unwrap();

    // Bounded output capture with timeout
    let result = tokio::time::timeout(timeout, async {
        let mut stdout_buf = vec![0u8; max_output_bytes];
        let mut stderr_buf = vec![0u8; max_output_bytes];

        let stdout_read = stdout_handle.read(&mut stdout_buf);
        let stderr_read = stderr_handle.read(&mut stderr_buf);

        let (stdout_n, stderr_n) = tokio::join!(stdout_read, stderr_read);
        let stdout_n = stdout_n.unwrap_or(0);
        let stderr_n = stderr_n.unwrap_or(0);

        let status = child.wait().await;

        (stdout_buf, stdout_n, stderr_buf, stderr_n, status)
    }).await;

    match result {
        Ok((stdout_buf, stdout_n, stderr_buf, stderr_n, status)) => {
            let stdout_truncated = stdout_n >= max_output_bytes;
            let stderr_truncated = stderr_n >= max_output_bytes;

            // UTF-8 validation (P0 is text-only)
            let stdout = String::from_utf8(stdout_buf[..stdout_n].to_vec())
                .map_err(|_| AmaError::ServiceUnavailable {
                    message: "stdout contains non-UTF-8 data".into(),
                })?;
            let stderr = String::from_utf8(stderr_buf[..stderr_n].to_vec())
                .map_err(|_| AmaError::ServiceUnavailable {
                    message: "stderr contains non-UTF-8 data".into(),
                })?;

            let exit_code = status
                .map_err(|e| AmaError::ServiceUnavailable {
                    message: format!("wait failed: {}", e),
                })?
                .code()
                .unwrap_or(-1);

            Ok(ShellExecResult {
                stdout,
                stderr,
                exit_code,
                truncated: stdout_truncated || stderr_truncated,
            })
        }
        Err(_) => {
            // Timeout — execute kill sequence
            // SIGTERM to entire process group
            if pid > 0 {
                unsafe { libc::kill(-pid, libc::SIGTERM); }
            }
            // Wait 2 seconds then SIGKILL
            tokio::time::sleep(Duration::from_secs(2)).await;
            if pid > 0 {
                unsafe { libc::kill(-pid, libc::SIGKILL); }
            }
            // Reap the child
            let _ = child.wait().await;

            Ok(ShellExecResult {
                stdout: String::new(),
                stderr: "process killed: timeout exceeded".into(),
                exit_code: -1,
                truncated: false,
            })
        }
    }
}
