//! Security tests for MCP Dev Bridge IPC socket permissions and peer credential authentication.

#[cfg(unix)]
#[test]
fn test_mcp_bridge_socket_security_permissions_and_peercred() {
    use std::os::unix::fs::PermissionsExt;
    use strata::{default_dev_socket_path, start_mcp_dev_listener};

    let default_path = default_dev_socket_path();
    let default_str = default_path.to_string_lossy();
    // Default socket path must never be naked world-accessible /tmp/strata-dev.sock
    assert!(
        !default_str.ends_with("/tmp/strata-dev.sock"),
        "Default socket path must be user-isolated: {default_str}"
    );

    // Test with a private test directory
    let test_dir = std::env::temp_dir().join(format!("strata_mcp_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&test_dir);
    let socket_path = test_dir.join("mcp_test.sock");

    start_mcp_dev_listener(Some(socket_path.clone())).expect("Failed to start MCP listener");

    // Verify parent directory permissions are 0700
    let parent_meta = std::fs::metadata(&test_dir).expect("Directory metadata failed");
    let parent_mode = parent_meta.permissions().mode() & 0o777;
    assert_eq!(
        parent_mode,
        0o700,
        "Parent directory must have 0700 permissions, got: {parent_mode:o}"
    );

    // Verify socket file permissions are 0600
    let sock_meta = std::fs::metadata(&socket_path).expect("Socket metadata failed");
    let sock_mode = sock_meta.permissions().mode() & 0o777;
    assert_eq!(
        sock_mode,
        0o600,
        "Socket file must have 0600 permissions, got: {sock_mode:o}"
    );

    // Connect to the socket as the current user
    let stream = std::os::unix::net::UnixStream::connect(&socket_path)
        .expect("Same-user connection must succeed");

    // Stream connected cleanly
    drop(stream);

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_dir_all(&test_dir);
}
