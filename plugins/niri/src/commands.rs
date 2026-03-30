//! Niri CLI command execution helpers.
//!
//! All commands use `std::process::Command` on background threads to avoid
//! depending on tokio's IO reactor (niri uses a Unix socket internally).

use anyhow::{Context, Result};
use std::process::Stdio;

/// Run a `niri msg` command with the given arguments and return stdout bytes.
///
/// Spawns the command on a background thread and waits for completion via
/// a flume channel.
pub async fn niri_msg(args: &[&str]) -> Result<Vec<u8>> {
    let args: Vec<String> = args.iter().map(std::string::ToString::to_string).collect();
    let (tx, rx) = flume::bounded(1);

    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new("niri");
        cmd.arg("msg");
        for arg in &args {
            cmd.arg(arg);
        }
        let result = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to execute niri msg");
        if tx.send(result).is_err() {
            log::warn!("[niri] command result receiver dropped");
        }
    });

    let output = rx
        .recv_async()
        .await
        .context("niri command thread cancelled")?
        .context("Failed to execute niri msg")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("niri msg failed: {stderr}");
    }

    Ok(output.stdout)
}

/// Run `niri msg --json <subcommand>` and parse the JSON response.
pub async fn niri_msg_json<T: serde::de::DeserializeOwned>(subcommand: &str) -> Result<T> {
    let stdout = niri_msg(&["--json", subcommand]).await?;
    serde_json::from_slice(&stdout).context("Failed to parse niri JSON response")
}

/// Run `niri msg action <args...>` to execute a Niri action.
pub async fn niri_action(args: &[&str]) -> Result<()> {
    let mut full_args = vec!["action"];
    full_args.extend_from_slice(args);
    niri_msg(&full_args).await?;
    Ok(())
}

/// Run `niri msg output <args...>` to configure an output.
pub async fn niri_output(args: &[&str]) -> Result<()> {
    let mut full_args = vec!["output"];
    full_args.extend_from_slice(args);
    niri_msg(&full_args).await?;
    Ok(())
}
