//! External command execution helpers for the portal.
//!
//! CONTRACT: these helpers are intentionally small and side-effect explicit.
//! They preserve stdout/stderr error text because readiness verification APIs
//! expose command failure diagnostics to operators.

use std::path::Path;
use std::process::Command;

pub(crate) fn run_in_dir(dir: &Path, command: &mut Command) -> std::result::Result<(), String> {
    let output = command
        .current_dir(dir)
        .output()
        .map_err(|err| format!("run command in {}: {err}", dir.display()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .trim()
        .to_string())
    }
}
