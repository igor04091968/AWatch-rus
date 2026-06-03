use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    about = "Rust launcher for the ActivityWatch-Russian browser smoke test",
    trailing_var_arg = true
)]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[arg(long)]
    script: Option<PathBuf>,

    #[arg(long)]
    node: Option<PathBuf>,

    #[arg(last = true)]
    args: Vec<OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LaunchPlan {
    node: PathBuf,
    script: PathBuf,
    args: Vec<OsString>,
    node_path: Option<OsString>,
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    let plan = build_launch_plan(&cli)?;
    run_child(&plan)
}

fn build_launch_plan(cli: &Cli) -> Result<LaunchPlan> {
    let root = cli.root.clone();
    let script = cli
        .script
        .clone()
        .unwrap_or_else(|| root.join("scripts").join("aw-webui-browser-smoke.mjs"));
    if !script.is_file() {
        bail!("browser smoke script not found: {}", script.display());
    }

    Ok(LaunchPlan {
        node: cli.node.clone().unwrap_or_else(|| PathBuf::from("node")),
        script,
        args: cli.args.clone(),
        node_path: default_node_path(std::env::var_os("NODE_PATH").as_deref()),
    })
}

fn default_node_path(current: Option<&OsStr>) -> Option<OsString> {
    if current.is_some_and(|value| !value.is_empty()) {
        return None;
    }
    let Some(home) = std::env::var_os("HOME") else {
        return None;
    };
    let path = PathBuf::from(home)
        .join(".agents")
        .join("skills")
        .join("playwright")
        .join("node_modules");
    if path.is_dir() {
        Some(path.as_os_str().to_os_string())
    } else {
        None
    }
}

fn run_child(plan: &LaunchPlan) -> Result<i32> {
    let mut command = Command::new(&plan.node);
    command.arg(&plan.script).args(&plan.args);
    if let Some(node_path) = &plan.node_path {
        command.env("NODE_PATH", node_path);
    }

    let status = command
        .status()
        .with_context(|| format!("run {}", plan.node.display()))?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn keeps_existing_node_path_untouched() {
        assert_eq!(
            default_node_path(Some(OsStr::new("/custom/node_modules"))),
            None
        );
    }

    #[test]
    fn launch_plan_uses_default_script_and_passes_args() {
        let dir = tempfile::tempdir().unwrap();
        let scripts = dir.path().join("scripts");
        fs::create_dir(&scripts).unwrap();
        let script = scripts.join("aw-webui-browser-smoke.mjs");
        fs::write(&script, "console.log('ok')\n").unwrap();

        let cli = Cli {
            root: dir.path().to_path_buf(),
            script: None,
            node: Some(PathBuf::from("/usr/bin/node")),
            args: vec![OsString::from("--probe"), OsString::from("value")],
        };

        let plan = build_launch_plan(&cli).unwrap();
        assert_eq!(plan.node, PathBuf::from("/usr/bin/node"));
        assert_eq!(plan.script, script);
        assert_eq!(
            plan.args,
            vec![OsString::from("--probe"), OsString::from("value")]
        );
    }

    #[test]
    fn missing_script_is_an_error() {
        let cli = Cli {
            root: PathBuf::from("/tmp/no-such-aw-browser-root"),
            script: None,
            node: None,
            args: vec![],
        };
        let err = build_launch_plan(&cli).unwrap_err().to_string();
        assert!(err.contains("browser smoke script not found"));
    }
}
