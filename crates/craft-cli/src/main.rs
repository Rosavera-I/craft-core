use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use craft_core::{
    CraftError, CraftHome, GithubSource, HarnessManager, ValidationResult, compose_harnesses,
    plan_composition, test_installed_harness, validate_harness_project,
};
use craft_memory::{Memory, MemoryError, MemoryScope};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(code) = error.code() {
                eprintln!("error[{code}]: {error}");
            } else {
                eprintln!("error: {error}");
            }
            ExitCode::from(1)
        }
    }
}

#[derive(Debug)]
enum CliError {
    Usage(String),
    Core(CraftError),
    Memory(MemoryError),
    Io { message: String, source: io::Error },
    Runtime(String),
}

impl CliError {
    fn code(&self) -> Option<&'static str> {
        match self {
            CliError::Usage(_) => None,
            CliError::Core(error) => Some(error.code()),
            CliError::Memory(error) => Some(error.code()),
            CliError::Io { .. } => Some("io"),
            CliError::Runtime(_) => Some("runtime"),
        }
    }

    fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    fn io(message: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            message: message.into(),
            source,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Usage(message) | CliError::Runtime(message) => write!(f, "{message}"),
            CliError::Core(error) => write!(f, "{error}"),
            CliError::Memory(error) => write!(f, "{error}"),
            CliError::Io { message, .. } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CliError::Core(error) => Some(error),
            CliError::Memory(error) => Some(error),
            CliError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<CraftError> for CliError {
    fn from(value: CraftError) -> Self {
        Self::Core(value)
    }
}

impl From<MemoryError> for CliError {
    fn from(value: MemoryError) -> Self {
        Self::Memory(value)
    }
}

impl From<io::Error> for CliError {
    fn from(value: io::Error) -> Self {
        Self::io(value.to_string(), value)
    }
}

impl From<String> for CliError {
    fn from(value: String) -> Self {
        Self::usage(value)
    }
}

fn run(args: Vec<String>) -> Result<(), CliError> {
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
            init_project(&target).map_err(Into::into)
        }
        Some("doctor") => {
            doctor();
            Ok(())
        }
        Some("harness") => harness_command(&args[1..]),
        Some("compose") => compose_command(&args[1..]),
        Some("compose-plan") => compose_plan_command(&args[1..]),
        Some("run") => run_compose_command(&args[1..]),
        Some("lsp") => lsp_command(),
        Some("validate") => validate_command(&args[1..]),
        Some("memory") => memory_command(&args[1..]),
        Some(command) => Err(CliError::usage(format!(
            "unknown command `{command}`\n\nRun `craft --help`."
        ))),
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
  craft compose <harness> [harness...] [-o craft.compose.toml] [--plan]
  craft compose-plan <harness> [harness...]
  craft run [craft.compose.toml] --model <model> [--runtime ollama] [--prompt <text>]
  craft lsp             Start the craft.toml language server on stdio
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

fn harness_command(args: &[String]) -> Result<(), CliError> {
    let manager = HarnessManager::new(CraftHome::from_env()?);
    match args.first().map(String::as_str) {
        Some("install") => {
            let source = args.get(1).ok_or_else(|| {
                CliError::usage("usage: craft harness install github:owner/repo[@ref]")
            })?;
            let source = GithubSource::parse(source)?;
            let result = manager.install_github(&source)?;
            println!(
                "installed {} {} from {}",
                result.harness.name, result.harness.version, result.harness.source
            );
            Ok(())
        }
        Some("list") => {
            let registry = manager.registry()?;
            let harnesses = registry.list()?;
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
                .ok_or_else(|| CliError::usage("usage: craft harness info <name>"))?;
            let registry = manager.registry()?;
            let harness = registry.info(name)?;
            println!("name: {}", harness.name);
            println!("version: {}", harness.version);
            println!("source: {}", harness.source);
            println!("path: {}", harness.path.display());
            Ok(())
        }
        Some("test") => {
            let name = args
                .get(1)
                .ok_or_else(|| CliError::usage("usage: craft harness test <name>"))?;
            let registry = manager.registry()?;
            let result = test_installed_harness(&registry, name)?;
            print_validation_result(&result);
            Ok(())
        }
        Some("uninstall") => {
            let name = args
                .get(1)
                .ok_or_else(|| CliError::usage("usage: craft harness uninstall <name>"))?;
            let registry = manager.registry()?;
            let harness = registry.uninstall(name, true)?;
            println!("uninstalled {}", harness.name);
            Ok(())
        }
        _ => Err(CliError::usage(
            "usage: craft harness <install|list|info|test|uninstall>",
        )),
    }
}

fn validate_command(args: &[String]) -> Result<(), CliError> {
    let root = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let result = validate_harness_project(root)?;
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

fn compose_command(args: &[String]) -> Result<(), CliError> {
    if args.is_empty() {
        return Err(CliError::usage(
            "usage: craft compose <harness> [harness...] [-o craft.compose.toml] [--plan]",
        ));
    }

    let mut names = Vec::new();
    let mut output = PathBuf::from("craft.compose.toml");
    let mut show_plan = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--plan" | "--dry-run" => {
                show_plan = true;
                index += 1;
            }
            "-o" | "--output" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| CliError::usage("craft compose -o requires a path"))?;
                output = PathBuf::from(value);
                index += 2;
            }
            value => {
                names.push(value.to_string());
                index += 1;
            }
        }
    }

    let manager = HarnessManager::new(CraftHome::from_env()?);
    let registry = manager.registry()?;
    if show_plan {
        let plan = plan_composition(&registry, &names)?;
        print_composition_plan(&plan);
        return Ok(());
    }

    let result = compose_harnesses(&registry, &names, &output)?;
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }
    println!("wrote {}", result.output_path.display());
    Ok(())
}

fn compose_plan_command(args: &[String]) -> Result<(), CliError> {
    if args.is_empty() {
        return Err(CliError::usage(
            "usage: craft compose-plan <harness> [harness...]",
        ));
    }

    let mut names = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--plan" | "--dry-run" => {
                index += 1;
            }
            "-o" | "--output" => {
                return Err(CliError::usage(
                    "craft compose-plan never writes output; remove -o/--output",
                ));
            }
            value => {
                names.push(value.to_string());
                index += 1;
            }
        }
    }

    let manager = HarnessManager::new(CraftHome::from_env()?);
    let registry = manager.registry()?;
    let plan = plan_composition(&registry, &names)?;
    print_composition_plan(&plan);
    Ok(())
}

fn print_composition_plan(plan: &craft_core::CompositionPlan) {
    println!("composition plan");
    println!("strategy: {}", plan.strategy);
    println!("harnesses:");
    for harness in &plan.harnesses {
        println!(
            "- {} {} ({})",
            harness.name, harness.version, harness.source
        );
        println!("  path: {}", harness.path.display());
        println!("  prompt: {}", harness.prompt_path.display());
        println!("  memory: {}", harness.memory_schema_path.display());
        println!("  tools: {}", harness.mcp_tools_path.display());
        println!("  validators: {}", harness.tdd_validators_path.display());
    }
    println!("merge:");
    println!("- prompts.system: concatenated in listed order");
    println!("- memory.schemas: namespaced by harness name");
    println!("- tools.mcp: namespaced by harness name");
    println!("- validators.tdd: namespaced by harness name");
    if !plan.warnings.is_empty() {
        println!("warnings:");
        for warning in &plan.warnings {
            println!("- {warning}");
        }
    }
}

fn run_compose_command(args: &[String]) -> Result<(), CliError> {
    let compose_path = args
        .first()
        .filter(|value| !value.starts_with('-'))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("craft.compose.toml"));
    let model = required_flag(args, "--model")?;
    let runtime = optional_flag(args, "--runtime").unwrap_or_else(|| "ollama".to_string());
    let user_prompt = optional_flag(args, "--prompt");
    let system_prompt = load_compose_system_prompt(&compose_path)?;
    let prompt = runtime_prompt(&system_prompt, user_prompt.as_deref());

    let status = Command::new(&runtime)
        .arg("run")
        .arg(&model)
        .arg(prompt)
        .status()
        .map_err(|err| CliError::io(format!("failed to run {runtime}: {err}"), err))?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::Runtime(format!(
            "{runtime} exited with status {status}"
        )))
    }
}

fn lsp_command() -> Result<(), CliError> {
    craft_lsp::run_server(io::stdin().lock(), io::stdout().lock()).map_err(Into::into)
}

fn load_compose_system_prompt(path: &Path) -> Result<String, CliError> {
    let contents = fs::read_to_string(path)
        .map_err(|err| CliError::io(format!("failed to read {}: {err}", path.display()), err))?;
    let mut section = String::new();
    for raw_line in contents.lines() {
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            section = name.trim().to_string();
            continue;
        }
        if section == "prompts" {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() == "system" {
                return parse_toml_string(value.trim()).ok_or_else(|| {
                    CliError::usage("compose prompts.system must be a quoted string")
                });
            }
        }
    }
    Err(CliError::usage("compose file is missing [prompts].system"))
}

fn runtime_prompt(system_prompt: &str, user_prompt: Option<&str>) -> String {
    match user_prompt {
        Some(prompt) if !prompt.trim().is_empty() => {
            format!("System prompt:\n{system_prompt}\n\nUser prompt:\n{prompt}")
        }
        _ => system_prompt.to_string(),
    }
}

fn strip_toml_comment(line: &str) -> &str {
    let mut escaped = false;
    let mut in_string = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_toml_string(value: &str) -> Option<String> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut output = String::new();
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match characters.next()? {
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            '"' => output.push('"'),
            '\\' => output.push('\\'),
            other => {
                output.push('\\');
                output.push(other);
            }
        }
    }
    Some(output)
}

fn memory_command(args: &[String]) -> Result<(), CliError> {
    let memory = Memory::from_env()?;
    match args.first().map(String::as_str) {
        Some("log") => {
            let scope = positional(args, 1, "usage: craft memory log <scope> <key> <value>")?;
            let key = positional(args, 2, "usage: craft memory log <scope> <key> <value>")?;
            let value = positional(args, 3, "usage: craft memory log <scope> <key> <value>")?;
            let scope = MemoryScope::parse(&scope)?;
            let fact = memory.record(scope, &key, &value)?;
            println!("recorded {}\t{}\t{}", fact.scope, fact.key, fact.value);
            Ok(())
        }
        Some("record") => {
            let scope = required_flag(args, "--scope")?;
            let key = required_flag(args, "--key")?;
            let value = required_flag(args, "--value")?;
            let scope = MemoryScope::parse(&scope)?;
            let fact = memory.record(scope, &key, &value)?;
            println!("recorded {}\t{}\t{}", fact.scope, fact.key, fact.value);
            Ok(())
        }
        Some("inspect") => {
            let scope = required_flag(args, "--scope")?;
            let scope = MemoryScope::parse(&scope)?;
            print_facts(memory.inspect(&scope)?);
            Ok(())
        }
        Some("recall") => {
            if args.get(1).is_some_and(|value| !value.starts_with('-')) {
                let scope = positional(args, 1, "usage: craft memory recall <scope> <key>")?;
                let key = positional(args, 2, "usage: craft memory recall <scope> <key>")?;
                let scope = MemoryScope::parse(&scope)?;
                print_optional_fact(memory.recall_key(&scope, &key)?);
            } else {
                let scope = required_flag(args, "--scope")?;
                let query = required_flag(args, "--query")?;
                let scope = MemoryScope::parse(&scope)?;
                print_facts(memory.recall(&scope, &query)?);
            }
            Ok(())
        }
        Some("search") => {
            let query = required_flag(args, "--query")?;
            let scopes = scope_flags(args)?;
            print_facts(memory.search(&query, &scopes)?);
            Ok(())
        }
        Some("context") => {
            let query = optional_flag(args, "--query").unwrap_or_default();
            let tokens = optional_flag(args, "--tokens")
                .map(|value| {
                    value.parse::<usize>().map_err(|_| {
                        CliError::usage("craft memory context --tokens must be a number")
                    })
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
            let items = memory.assemble_context(&scopes, query, tokens)?;
            for item in items {
                println!("{}\t{}\t{}", item.scope, item.key, item.value);
            }
            Ok(())
        }
        _ => Err(CliError::usage(
            "usage: craft memory <log|record|inspect|recall|search|context> [options]",
        )),
    }
}

fn positional(args: &[String], index: usize, usage: &str) -> Result<String, CliError> {
    args.get(index)
        .cloned()
        .ok_or_else(|| CliError::usage(usage))
}

fn required_flag(args: &[String], name: &str) -> Result<String, CliError> {
    optional_flag(args, name).ok_or_else(|| CliError::usage(format!("{name} is required")))
}

fn optional_flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].clone())
}

fn scope_flags(args: &[String]) -> Result<Vec<MemoryScope>, CliError> {
    let mut scopes = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--scope" {
            let value = args
                .get(index + 1)
                .ok_or_else(|| CliError::usage("--scope requires a value"))?;
            scopes.push(MemoryScope::parse(value)?);
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
