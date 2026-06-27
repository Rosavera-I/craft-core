use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use craft_core::{
    CraftHome, GithubSource, HarnessManager, ValidationResult, compose_harnesses,
    test_installed_harness, validate_harness_project,
};
use craft_memory::{Memory, MemoryScope};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(1)
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None | Some("--help" | "-h" | "help") => {
            print_help();
            Ok(())
        }
        Some("--version" | "-V" | "version") => {
            println!("craft {VERSION}");
            Ok(())
        }
        Some("init") => {
            let target = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            init_project(&target).map_err(|err| err.to_string())
        }
        Some("doctor") => {
            doctor();
            Ok(())
        }
        Some("harness") => harness_command(&args[1..]),
        Some("compose") => compose_command(&args[1..]),
        Some("validate") => validate_command(&args[1..]),
        Some("memory") => memory_command(&args[1..]),
        Some(command) => Err(format!(
            "unknown command `{command}`\n\nRun `craft --help`."
        )),
    }
}

fn print_help() {
    println!(
        "\
craft {VERSION}

Usage:
  craft init [path]      Create a CRAFT project scaffold
  craft doctor           Check local development environment
  craft harness install github:owner/repo[@ref]
  craft harness list
  craft harness info <name>
  craft harness test <name>
  craft harness uninstall <name>
  craft compose <harness> [harness...] [-o craft.compose.toml]
  craft validate [path]   Validate a harness manifest and TDD checks
  craft memory log <scope> <key> <value>
  craft memory recall <scope> <key>
  craft memory record --scope <scope> --key <key> --value <value>
  craft memory inspect --scope <scope>
  craft memory search --query <query> [--scope <scope>...]
  craft memory context --query <query> [--tokens 1200] [--scope <scope>...]
  craft version          Print version
  craft --help           Print help
"
    );
}

fn init_project(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root.join(".craft"))?;
    fs::create_dir_all(root.join("prompts"))?;
    fs::create_dir_all(root.join("memory"))?;
    fs::create_dir_all(root.join("tools"))?;
    fs::create_dir_all(root.join("validators"))?;

    write_if_missing(
        root.join("prompts/system.md"),
        "You are a focused CRAFT harness.\n",
    )?;
    write_if_missing(
        root.join("memory/schema.toml"),
        "# memory schema placeholder\n",
    )?;
    write_if_missing(
        root.join("tools/mcp.toml"),
        "# MCP tool bindings placeholder\n",
    )?;
    write_if_missing(
        root.join("validators/checks.tdd"),
        "# tdd-dsl validators placeholder\n",
    )?;
    write_if_missing(root.join("craft.toml"), default_manifest())?;

    println!("initialized CRAFT project at {}", root.display());
    Ok(())
}

fn write_if_missing(path: impl AsRef<Path>, contents: &str) -> io::Result<()> {
    let path = path.as_ref();
    if !path.exists() {
        fs::write(path, contents)?;
    }
    Ok(())
}

fn default_manifest() -> &'static str {
    r#"[harness]
name = "starter-harness"
version = "0.1.0"
description = "A starter CRAFT expertise harness"
authors = ["JMoak"]

[model]
min_context = 4096
recommended = ["llama3.1:8b", "qwen2.5:7b"]

[prompts]
system = "prompts/system.md"

[memory]
schema = "memory/schema.toml"

[tools]
mcp = "tools/mcp.toml"

[validators]
tdd = "validators/checks.tdd"
"#
}

fn doctor() {
    println!("CRAFT doctor");
    println!("  rustc: {}", probe_version("rustc", "--version"));
    println!("  cargo: {}", probe_version("cargo", "--version"));
    println!("  git:   {}", probe_version("git", "--version"));
    println!("  ollama: {}", probe_version("ollama", "--version"));
}

fn harness_command(args: &[String]) -> Result<(), String> {
    let manager = HarnessManager::new(CraftHome::from_env().map_err(|err| err.to_string())?);
    match args.first().map(String::as_str) {
        Some("install") => {
            let source = args.get(1).ok_or_else(|| {
                "usage: craft harness install github:owner/repo[@ref]".to_string()
            })?;
            let source = GithubSource::parse(source).map_err(|err| err.to_string())?;
            let result = manager
                .install_github(&source)
                .map_err(|err| err.to_string())?;
            println!(
                "installed {} {} from {}",
                result.harness.name, result.harness.version, result.harness.source
            );
            Ok(())
        }
        Some("list") => {
            let registry = manager.registry().map_err(|err| err.to_string())?;
            let harnesses = registry.list().map_err(|err| err.to_string())?;
            if harnesses.is_empty() {
                println!("no harnesses installed");
            } else {
                for harness in harnesses {
                    println!(
                        "{}\t{}\t{}\t{}",
                        harness.name,
                        harness.version,
                        harness.source,
                        harness.path.display()
                    );
                }
            }
            Ok(())
        }
        Some("info") => {
            let name = args
                .get(1)
                .ok_or_else(|| "usage: craft harness info <name>".to_string())?;
            let registry = manager.registry().map_err(|err| err.to_string())?;
            let harness = registry.info(name).map_err(|err| err.to_string())?;
            println!("name: {}", harness.name);
            println!("version: {}", harness.version);
            println!("source: {}", harness.source);
            println!("path: {}", harness.path.display());
            Ok(())
        }
        Some("test") => {
            let name = args
                .get(1)
                .ok_or_else(|| "usage: craft harness test <name>".to_string())?;
            let registry = manager.registry().map_err(|err| err.to_string())?;
            let result = test_installed_harness(&registry, name).map_err(|err| err.to_string())?;
            print_validation_result(&result);
            Ok(())
        }
        Some("uninstall") => {
            let name = args
                .get(1)
                .ok_or_else(|| "usage: craft harness uninstall <name>".to_string())?;
            let registry = manager.registry().map_err(|err| err.to_string())?;
            let harness = registry
                .uninstall(name, true)
                .map_err(|err| err.to_string())?;
            println!("uninstalled {}", harness.name);
            Ok(())
        }
        _ => Err("usage: craft harness <install|list|info|test|uninstall>".to_string()),
    }
}

fn validate_command(args: &[String]) -> Result<(), String> {
    let root = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let result = validate_harness_project(root).map_err(|err| err.to_string())?;
    print_validation_result(&result);
    Ok(())
}

fn print_validation_result(result: &ValidationResult) {
    println!("validated {}", result.harness_name);
    println!("manifest: ok");
    if result.checks_run {
        let runner = result.runner.as_deref().unwrap_or("tdd-dsl");
        println!("tdd: ok ({runner}) {}", result.tdd_path.display());
    } else {
        println!("tdd: skipped (no checks) {}", result.tdd_path.display());
    }
}

fn compose_command(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err(
            "usage: craft compose <harness> [harness...] [-o craft.compose.toml]".to_string(),
        );
    }

    let mut names = Vec::new();
    let mut output = PathBuf::from("craft.compose.toml");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-o" | "--output" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "craft compose -o requires a path".to_string())?;
                output = PathBuf::from(value);
                index += 2;
            }
            value => {
                names.push(value.to_string());
                index += 1;
            }
        }
    }

    let manager = HarnessManager::new(CraftHome::from_env().map_err(|err| err.to_string())?);
    let registry = manager.registry().map_err(|err| err.to_string())?;
    let result = compose_harnesses(&registry, &names, &output).map_err(|err| err.to_string())?;
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }
    println!("wrote {}", result.output_path.display());
    Ok(())
}

fn memory_command(args: &[String]) -> Result<(), String> {
    let memory = Memory::from_env().map_err(|err| err.to_string())?;
    match args.first().map(String::as_str) {
        Some("log") => {
            let scope = positional(args, 1, "usage: craft memory log <scope> <key> <value>")?;
            let key = positional(args, 2, "usage: craft memory log <scope> <key> <value>")?;
            let value = positional(args, 3, "usage: craft memory log <scope> <key> <value>")?;
            let scope = MemoryScope::parse(&scope).map_err(|err| err.to_string())?;
            let fact = memory
                .record(scope, &key, &value)
                .map_err(|err| err.to_string())?;
            println!("recorded {}\t{}\t{}", fact.scope, fact.key, fact.value);
            Ok(())
        }
        Some("record") => {
            let scope = required_flag(args, "--scope")?;
            let key = required_flag(args, "--key")?;
            let value = required_flag(args, "--value")?;
            let scope = MemoryScope::parse(&scope).map_err(|err| err.to_string())?;
            let fact = memory
                .record(scope, &key, &value)
                .map_err(|err| err.to_string())?;
            println!("recorded {}\t{}\t{}", fact.scope, fact.key, fact.value);
            Ok(())
        }
        Some("inspect") => {
            let scope = required_flag(args, "--scope")?;
            let scope = MemoryScope::parse(&scope).map_err(|err| err.to_string())?;
            print_facts(memory.inspect(&scope).map_err(|err| err.to_string())?);
            Ok(())
        }
        Some("recall") => {
            if args.get(1).is_some_and(|value| !value.starts_with('-')) {
                let scope = positional(args, 1, "usage: craft memory recall <scope> <key>")?;
                let key = positional(args, 2, "usage: craft memory recall <scope> <key>")?;
                let scope = MemoryScope::parse(&scope).map_err(|err| err.to_string())?;
                print_optional_fact(
                    memory
                        .recall_key(&scope, &key)
                        .map_err(|err| err.to_string())?,
                );
            } else {
                let scope = required_flag(args, "--scope")?;
                let query = required_flag(args, "--query")?;
                let scope = MemoryScope::parse(&scope).map_err(|err| err.to_string())?;
                print_facts(
                    memory
                        .recall(&scope, &query)
                        .map_err(|err| err.to_string())?,
                );
            }
            Ok(())
        }
        Some("search") => {
            let query = required_flag(args, "--query")?;
            let scopes = scope_flags(args)?;
            print_facts(
                memory
                    .search(&query, &scopes)
                    .map_err(|err| err.to_string())?,
            );
            Ok(())
        }
        Some("context") => {
            let query = optional_flag(args, "--query").unwrap_or_default();
            let tokens = optional_flag(args, "--tokens")
                .map(|value| {
                    value
                        .parse::<usize>()
                        .map_err(|_| "craft memory context --tokens must be a number".to_string())
                })
                .transpose()?
                .unwrap_or(1200);
            let mut scopes = scope_flags(args)?;
            if scopes.is_empty() {
                scopes = vec![
                    MemoryScope::Global,
                    MemoryScope::User,
                    MemoryScope::Project,
                    MemoryScope::Session,
                ];
            }
            let items = memory
                .assemble_context(&scopes, query, tokens)
                .map_err(|err| err.to_string())?;
            for item in items {
                println!("{}\t{}\t{}", item.scope, item.key, item.value);
            }
            Ok(())
        }
        _ => Err(
            "usage: craft memory <log|record|inspect|recall|search|context> [options]".to_string(),
        ),
    }
}

fn positional(args: &[String], index: usize, usage: &str) -> Result<String, String> {
    args.get(index).cloned().ok_or_else(|| usage.to_string())
}

fn required_flag(args: &[String], name: &str) -> Result<String, String> {
    optional_flag(args, name).ok_or_else(|| format!("{name} is required"))
}

fn optional_flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].clone())
}

fn scope_flags(args: &[String]) -> Result<Vec<MemoryScope>, String> {
    let mut scopes = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--scope" {
            let value = args
                .get(index + 1)
                .ok_or_else(|| "--scope requires a value".to_string())?;
            scopes.push(MemoryScope::parse(value).map_err(|err| err.to_string())?);
            index += 2;
        } else {
            index += 1;
        }
    }
    Ok(scopes)
}

fn print_facts(facts: Vec<craft_memory::MemoryFact>) {
    if facts.is_empty() {
        println!("no memory facts found");
    } else {
        for fact in facts {
            println!("{}\t{}\t{}", fact.scope, fact.key, fact.value);
        }
    }
}

fn print_optional_fact(fact: Option<craft_memory::MemoryFact>) {
    match fact {
        Some(fact) => println!("{}\t{}\t{}", fact.scope, fact.key, fact.value),
        None => println!("no memory facts found"),
    }
}

fn probe_version(binary: &str, arg: &str) -> String {
    match Command::new(binary).arg(arg).output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let value = if stdout.trim().is_empty() {
                stderr.trim()
            } else {
                stdout.trim()
            };
            value.to_string()
        }
        Ok(_) => "found, but version check failed".to_string(),
        Err(_) => "not found".to_string(),
    }
}
