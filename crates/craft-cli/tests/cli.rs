use std::fs;
use std::path::{Path, PathBuf};
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

#[test]
fn harness_list_handles_empty_registry() {
    let root = temp_root("craft-cli-list");
    let output = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("harness")
        .arg("list")
        .env("CRAFT_HOME", &root)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("no harnesses installed"));

    fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn compose_command_writes_compose_file() {
    let root = temp_root("craft-cli-compose");
    let craft_home = root.join(".craft");
    let harness_root = craft_home.join("harnesses/godot-designer");
    create_harness(&harness_root, "godot-designer");
    seed_registry(&craft_home, "godot-designer", &harness_root);

    let output_path = root.join("craft.compose.toml");
    let output = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("compose")
        .arg("godot-designer")
        .arg("--output")
        .arg(&output_path)
        .env("CRAFT_HOME", &craft_home)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let contents = fs::read_to_string(output_path).unwrap_or_else(|err| panic!("{err}"));
    assert!(contents.contains("harnesses = [\"godot-designer\"]"));

    fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn memory_commands_record_and_inspect_facts() {
    let root = temp_root("craft-cli-memory");

    let record = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("memory")
        .arg("record")
        .arg("--scope")
        .arg("project")
        .arg("--key")
        .arg("language")
        .arg("--value")
        .arg("rust")
        .env("CRAFT_HOME", &root)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(
        record.status.success(),
        "{}",
        String::from_utf8_lossy(&record.stderr)
    );
    assert!(String::from_utf8_lossy(&record.stdout).contains("recorded project"));

    let inspect = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("memory")
        .arg("inspect")
        .arg("--scope")
        .arg("project")
        .env("CRAFT_HOME", &root)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(
        inspect.status.success(),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("project\tlanguage\trust"));

    let search = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("memory")
        .arg("search")
        .arg("--query")
        .arg("rust")
        .arg("--scope")
        .arg("project")
        .env("CRAFT_HOME", &root)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(
        search.status.success(),
        "{}",
        String::from_utf8_lossy(&search.stderr)
    );
    assert!(String::from_utf8_lossy(&search.stdout).contains("project\tlanguage\trust"));

    assert!(root.join("memory.sqlite3").is_file());
    assert!(root.join("logs").is_dir());

    fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
}

fn temp_root(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|err| panic!("{err}"))
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{unique}"))
}

fn create_harness(root: &Path, name: &str) {
    fs::create_dir_all(root.join("prompts")).unwrap_or_else(|err| panic!("{err}"));
    fs::create_dir_all(root.join("memory")).unwrap_or_else(|err| panic!("{err}"));
    fs::create_dir_all(root.join("tools")).unwrap_or_else(|err| panic!("{err}"));
    fs::create_dir_all(root.join("validators")).unwrap_or_else(|err| panic!("{err}"));
    fs::write(root.join("prompts/system.md"), "").unwrap_or_else(|err| panic!("{err}"));
    fs::write(root.join("memory/schema.toml"), "").unwrap_or_else(|err| panic!("{err}"));
    fs::write(root.join("tools/mcp.toml"), "").unwrap_or_else(|err| panic!("{err}"));
    fs::write(root.join("validators/checks.tdd"), "").unwrap_or_else(|err| panic!("{err}"));
    fs::write(
        root.join("craft.toml"),
        format!(
            r#"[harness]
name = "{name}"
version = "0.1.0"
description = "CLI test harness"
authors = ["JMoak"]

[model]
min_context = 4096
recommended = ["llama3.1:8b"]

[prompts]
system = "prompts/system.md"

[memory]
schema = "memory/schema.toml"

[tools]
mcp = "tools/mcp.toml"

[validators]
tdd = "validators/checks.tdd"
"#
        ),
    )
    .unwrap_or_else(|err| panic!("{err}"));
}

fn seed_registry(craft_home: &Path, name: &str, harness_root: &Path) {
    fs::create_dir_all(craft_home).unwrap_or_else(|err| panic!("{err}"));
    let db = craft_home.join("registry.sqlite3");
    let create_status = Command::new("sqlite3")
        .arg(&db)
        .arg(
            "CREATE TABLE harnesses (
                name TEXT PRIMARY KEY,
                version TEXT NOT NULL,
                source TEXT NOT NULL,
                path TEXT NOT NULL,
                installed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .status()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(create_status.success());

    let insert_status = Command::new("sqlite3")
        .arg(&db)
        .arg(format!(
            "INSERT INTO harnesses (name, version, source, path) VALUES ('{name}', '0.1.0', 'github:JMoak/{name}', '{}');",
            harness_root.display()
        ))
        .status()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(insert_status.success());
}
