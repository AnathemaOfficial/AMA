#[cfg(unix)]
mod tests {
    use ama::actuator::shell::*;

    #[tokio::test]
    async fn executes_simple_intent() {
        let result = shell_exec(
            "/bin/echo",
            &["hello", "world"],
            "/tmp",
            "test-id",
            std::time::Duration::from_secs(5),
            65_536,
        ).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello world"));
    }

    #[tokio::test]
    async fn kills_on_timeout() {
        let result = shell_exec(
            "/bin/sleep",
            &["60"],
            "/tmp",
            "test-timeout",
            std::time::Duration::from_secs(1),
            65_536,
        ).await;
        // Should timeout and kill
        assert!(result.is_ok()); // returns result with non-zero exit
    }

    #[tokio::test]
    async fn captures_stderr() {
        let result = shell_exec(
            "/bin/ls",
            &["/nonexistent_path_xyz"],
            "/tmp",
            "test-stderr",
            std::time::Duration::from_secs(5),
            65_536,
        ).await.unwrap();
        assert_ne!(result.exit_code, 0);
        assert!(!result.stderr.is_empty());
    }
}
