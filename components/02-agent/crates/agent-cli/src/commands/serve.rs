use agent_core::Result;
use std::process::Command;

pub fn execute() -> Result<()> {
    println!("Starting 02-agentd daemon...");
    let status = Command::new("02-agentd").status().or_else(|_| {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let daemon_bin = parent.join("02-agentd");
                if daemon_bin.exists() {
                    return Command::new(daemon_bin).status();
                }
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "02-agentd binary not found on PATH or alongside 02agent",
        ))
    })?;

    if !status.success() {
        eprintln!("02-agentd exited with code: {:?}", status.code());
    }
    Ok(())
}
