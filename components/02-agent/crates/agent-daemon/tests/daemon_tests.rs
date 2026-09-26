use agent_daemon::{DaemonSocket, RepoWatcher};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use tempfile::tempdir;

#[test]
fn test_socket_bind_and_permissions() {
    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("test_agent.sock");

    let listener = DaemonSocket::bind(&sock_path).unwrap();
    assert!(sock_path.exists());

    // Check permissions are 0600 (user read/write only)
    let meta = std::fs::metadata(&sock_path).unwrap();
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    // Test client connect
    let mut client = UnixStream::connect(&sock_path).unwrap();
    let (mut server_stream, _) = listener.accept().unwrap();

    // Client writes message
    writeln!(
        client,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}}"
    )
    .unwrap();
    client.flush().unwrap();

    // Server reads message
    let mut reader = BufReader::new(&mut server_stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("ping"));

    // Server responds
    writeln!(
        server_stream,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{}}}}"
    )
    .unwrap();
    server_stream.flush().unwrap();

    // Client reads response
    let mut client_reader = BufReader::new(client);
    let mut resp = String::new();
    client_reader.read_line(&mut resp).unwrap();
    assert!(resp.contains("\"result\":{}"));

    // Cleanup
    drop(listener);
    DaemonSocket::cleanup(&sock_path);
    assert!(!sock_path.exists());
}

#[test]
fn test_repo_watcher_state() {
    let dir = tempdir().unwrap();
    let dummy_repo1 = dir.path().join("repo1");
    let dummy_repo2 = dir.path().join("repo2");

    let watcher = RepoWatcher::new().unwrap();
    assert!(watcher.add(dummy_repo1.clone()).unwrap());
    assert!(watcher.add(dummy_repo2.clone()).unwrap());

    // Duplicate add returns false
    assert!(!watcher.add(dummy_repo1.clone()).unwrap());

    let list = watcher.list();
    assert!(list.contains(&dummy_repo1));
    assert!(list.contains(&dummy_repo2));

    // Remove repo1
    assert!(watcher.remove(&dummy_repo1).unwrap());
    let list_after = watcher.list();
    assert!(!list_after.contains(&dummy_repo1));
    assert!(list_after.contains(&dummy_repo2));

    // Cleanup
    watcher.remove(&dummy_repo2).unwrap();
}
