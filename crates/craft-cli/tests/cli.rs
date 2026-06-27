use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn version_command_prints_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("version")
        .output()
        .unwrap_or_else(|err| panic!("{err}"));

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("craft 0.1.0"));
}

#[test]
fn init_creates_project_scaffold() {
    let root = temp_root("craft-cli-init");
    let output = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("init")
        .arg(&root)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));

    assert!(output.status.success());
    assert!(root.join(".craft").is_dir());
    assert!(root.join("craft.toml").is_file());
    assert!(root.join("prompts/system.md").is_file());
    assert!(root.join("memory/schema.toml").is_file());
    assert!(root.join("tools/mcp.toml").is_file());
    assert!(root.join("validators/checks.tdd").is_file());

    fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn doctor_runs_without_panicking() {
    let output = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("doctor")
        .output()
        .unwrap_or_else(|err| panic!("{err}"));

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("CRAFT doctor"));
}

fn temp_root(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|err| panic!("{err}"))
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{unique}"))
}
