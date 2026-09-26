use agent_core::{paths::StoragePaths, AgentError, Result};
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub struct DaemonSocket;

impl DaemonSocket {
    /// Determine appropriate Unix domain socket path for user session.
    pub fn socket_path() -> Result<PathBuf> {
        if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR") {
            let path = PathBuf::from(runtime_dir).join("02agent.sock");
            return Ok(path);
        }

        let base = StoragePaths::data_dir()?;
        fs::create_dir_all(&base)?;
        Ok(base.join("02agent.sock"))
    }

    /// Bind to Unix domain socket, cleaning up stale sockets if present.
    pub fn bind(path: &PathBuf) -> Result<UnixListener> {
        if path.exists() {
            // Check if another daemon instance is actively listening
            if UnixStream::connect(path).is_ok() {
                return Err(AgentError::General(format!(
                    "Another 02-agentd daemon is already listening on {}",
                    path.display()
                )));
            }
            // Stale socket from previous run; safe to unlink
            let _ = fs::remove_file(path);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(path).map_err(|e| {
            AgentError::General(format!("Failed to bind socket {}: {}", path.display(), e))
        })?;

        // Ensure user-only permissions (0600)
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));

        Ok(listener)
    }

    /// Clean up socket file on shutdown.
    pub fn cleanup(path: &PathBuf) {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
}
