use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
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
    assert!(contents.contains("[prompts]"));
    assert!(contents.contains("System prompt for godot-designer"));
    assert!(contents.contains("[memory.schemas]"));
    assert!(contents.contains("[tools.mcp]"));
    assert!(contents.contains("[validators.tdd]"));

    fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn validate_accepts_initialized_project_without_checks() {
    let root = temp_root("craft-cli-validate-empty");
    let init = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("init")
        .arg(&root)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let validate = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("validate")
        .arg(&root)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(
        validate.status.success(),
        "{}",
        String::from_utf8_lossy(&validate.stderr)
    );
    let stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(stdout.contains("validated starter-harness"));
    assert!(stdout.contains("tdd: skipped"));

    fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn validate_runs_tdd_dsl_when_checks_exist() {
    let root = temp_root("craft-cli-validate-tdd");
    create_harness(&root, "godot-designer");
    let bin_dir = fake_tdd_dsl_bin(&root);

    let validate = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("validate")
        .arg(&root)
        .env("PATH", path_with_prefix(&bin_dir))
        .output()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(
        validate.status.success(),
        "{}",
        String::from_utf8_lossy(&validate.stderr)
    );
    let stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(stdout.contains("validated godot-designer"));
    assert!(stdout.contains("tdd: ok"));

    fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn harness_test_runs_installed_harness_tdd_checks() {
    let root = temp_root("craft-cli-harness-test");
    let craft_home = root.join(".craft");
    let harness_root = craft_home.join("harnesses/godot-designer");
    create_harness(&harness_root, "godot-designer");
    seed_registry(&craft_home, "godot-designer", &harness_root);
    let bin_dir = fake_tdd_dsl_bin(&root);

    let test = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("harness")
        .arg("test")
        .arg("godot-designer")
        .env("CRAFT_HOME", &craft_home)
        .env("PATH", path_with_prefix(&bin_dir))
        .output()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(
        test.status.success(),
        "{}",
        String::from_utf8_lossy(&test.stderr)
    );
    let stdout = String::from_utf8_lossy(&test.stdout);
    assert!(stdout.contains("validated godot-designer"));
    assert!(stdout.contains("tdd: ok"));

    fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn run_invokes_runtime_with_composed_system_prompt() {
    let root = temp_root("craft-cli-run");
    fs::create_dir_all(&root).unwrap_or_else(|err| panic!("{err}"));
    let compose_path = root.join("craft.compose.toml");
    fs::write(
        &compose_path,
        r#"[compose]
strategy = "ordered-merge"
harnesses = ["godot-designer"]

[prompts]
system = "System prompt for godot-designer\n"
"#,
    )
    .unwrap_or_else(|err| panic!("{err}"));
    let runtime = fake_runtime(&root);
    let capture_path = root.join("runtime-args.txt");

    let output = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("run")
        .arg(&compose_path)
        .arg("--model")
        .arg("llama3.1:8b")
        .arg("--runtime")
        .arg(&runtime)
        .env("CRAFT_TEST_RUNTIME_CAPTURE", &capture_path)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = fs::read_to_string(capture_path).unwrap_or_else(|err| panic!("{err}"));
    assert!(args.contains("run"));
    assert!(args.contains("llama3.1:8b"));
    assert!(args.contains("System prompt for godot-designer"));

    fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn run_combines_optional_user_prompt() {
    let root = temp_root("craft-cli-run-prompt");
    fs::create_dir_all(&root).unwrap_or_else(|err| panic!("{err}"));
    let compose_path = root.join("craft.compose.toml");
    fs::write(
        &compose_path,
        r#"[prompts]
system = "System prompt\n"
"#,
    )
    .unwrap_or_else(|err| panic!("{err}"));
    let runtime = fake_runtime(&root);
    let capture_path = root.join("runtime-args.txt");

    let output = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("run")
        .arg(&compose_path)
        .arg("--model")
        .arg("qwen2.5:7b")
        .arg("--runtime")
        .arg(&runtime)
        .arg("--prompt")
        .arg("Design a test plan")
        .env("CRAFT_TEST_RUNTIME_CAPTURE", &capture_path)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = fs::read_to_string(capture_path).unwrap_or_else(|err| panic!("{err}"));
    assert!(args.contains("System prompt:"));
    assert!(args.contains("User prompt:"));
    assert!(args.contains("Design a test plan"));

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

#[test]
fn memory_log_and_positional_recall_persist_across_processes() {
    let root = temp_root("craft-cli-memory-log");

    let log = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("memory")
        .arg("log")
        .arg("project")
        .arg("runtime")
        .arg("sqlite")
        .env("CRAFT_HOME", &root)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(
        log.status.success(),
        "{}",
        String::from_utf8_lossy(&log.stderr)
    );
    assert!(String::from_utf8_lossy(&log.stdout).contains("recorded project"));

    let recall = Command::new(env!("CARGO_BIN_EXE_craft"))
        .arg("memory")
        .arg("recall")
        .arg("project")
        .arg("runtime")
        .env("CRAFT_HOME", &root)
        .output()
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(
        recall.status.success(),
        "{}",
        String::from_utf8_lossy(&recall.stderr)
    );
    assert!(String::from_utf8_lossy(&recall.stdout).contains("project\truntime\tsqlite"));

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

fn fake_tdd_dsl_bin(root: &Path) -> PathBuf {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|err| panic!("{err}"));
    let binary = bin_dir.join("tdd-dsl");
    fs::write(
        &binary,
        "#!/usr/bin/env sh\nif [ \"$1\" = \"--version\" ] || [ \"$1\" = \"--help\" ]; then exit 0; fi\ntest -f \"$1\"\n",
    )
    .unwrap_or_else(|err| panic!("{err}"));
    let mut permissions = fs::metadata(&binary)
        .unwrap_or_else(|err| panic!("{err}"))
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&binary, permissions).unwrap_or_else(|err| panic!("{err}"));
    bin_dir
}

fn fake_runtime(root: &Path) -> PathBuf {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|err| panic!("{err}"));
    let binary = bin_dir.join("fake-runtime");
    fs::write(
        &binary,
        "#!/usr/bin/env sh\nprintf '%s\\n' \"$@\" > \"$CRAFT_TEST_RUNTIME_CAPTURE\"\n",
    )
    .unwrap_or_else(|err| panic!("{err}"));
    let mut permissions = fs::metadata(&binary)
        .unwrap_or_else(|err| panic!("{err}"))
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&binary, permissions).unwrap_or_else(|err| panic!("{err}"));
    binary
}

fn path_with_prefix(prefix: &Path) -> OsString {
    let mut paths = vec![prefix.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).unwrap_or_else(|err| panic!("{err}"))
}

fn create_harness(root: &Path, name: &str) {
    fs::create_dir_all(root.join("prompts")).unwrap_or_else(|err| panic!("{err}"));
    fs::create_dir_all(root.join("memory")).unwrap_or_else(|err| panic!("{err}"));
    fs::create_dir_all(root.join("tools")).unwrap_or_else(|err| panic!("{err}"));
    fs::create_dir_all(root.join("validators")).unwrap_or_else(|err| panic!("{err}"));
    fs::write(
        root.join("prompts/system.md"),
        format!("System prompt for {name}\n"),
    )
    .unwrap_or_else(|err| panic!("{err}"));
    fs::write(
        root.join("memory/schema.toml"),
        format!("[facts]\nowner = \"{name}\"\n"),
    )
    .unwrap_or_else(|err| panic!("{err}"));
    fs::write(
        root.join("tools/mcp.toml"),
        format!("[[server]]\nname = \"{name}-tools\"\n"),
    )
    .unwrap_or_else(|err| panic!("{err}"));
    fs::write(
        root.join("validators/checks.tdd"),
        format!("check {name}\n"),
    )
    .unwrap_or_else(|err| panic!("{err}"));
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
