use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitCode};

use clap::{Arg, ArgAction, Command};
use clap_complete::{generate, shells};
use craft_core::{
    ConflictStrategy, CraftError, CraftHome, GithubSource, HarnessManager, HarnessProject,
    InstalledHarness, ValidationResult, compose_harnesses, plan_composition,
    test_installed_harness, validate_harness_project,
};
use craft_memory::{Memory, MemoryError, MemoryScope};
use craft_registry::RegistryError;
use dialoguer::{Confirm, theme::ColorfulTheme};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::{Archive, Builder};

mod registry;
mod ui;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const LOCKFILE_NAME: &str = "craft.lock";
const LOCKFILE_VERSION: u32 = 1;

/// The project-local record of registry versions selected by `craft harness install`.
///
/// This file deliberately stores only immutable registry coordinates and the archive
/// digest. Local installation paths are machine-specific and remain in `CRAFT_HOME`.
#[derive(Debug, Default, Deserialize, Serialize)]
struct HarnessLockfile {
    version: u32,
    #[serde(default)]
    harness: Vec<LockedHarness>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LockedHarness {
    org: String,
    name: String,
    version: String,
    source: String,
    checksum: String,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            ui::error(error.to_string(), error.code());
            if let Some(suggestion) = error.suggestion() {
                ui::hint(suggestion);
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
    Registry(RegistryError),
    Io { message: String, source: io::Error },
    Dialoguer(dialoguer::Error),
    Manifest(craft_manifest::ManifestError),
    Runtime(String),
}

impl CliError {
    fn code(&self) -> Option<&'static str> {
        match self {
            CliError::Usage(_) => None,
            CliError::Core(error) => Some(error.code()),
            CliError::Memory(error) => Some(error.code()),
            CliError::Registry(error) => Some(error.error_code()),
            CliError::Io { .. } => Some("io"),
            CliError::Dialoguer(_) => Some("dialog"),
            CliError::Manifest(_) => Some("manifest"),
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

    fn suggestion(&self) -> Option<String> {
        match self {
            CliError::Usage(message) => {
                let command = message
                    .strip_prefix("unknown command `")?
                    .split('`')
                    .next()?;
                suggest_command(command)
                    .map(|suggestion| format!("did you mean `craft {suggestion}`?"))
            }
            CliError::Core(error) => Some(format!(
                "run `craft doctor` if the local environment may be missing dependencies; details: {}",
                error.code()
            )),
            CliError::Memory(error) => Some(format!(
                "check the memory command arguments and scope; details: {}",
                error.code()
            )),
            CliError::Registry(_) => Some(
                "run `craft login --api-key <key> [--registry <url>]` and try again".to_string(),
            ),
            CliError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound => {
                Some("check that the path or runtime exists and try again".to_string())
            }
            CliError::Io { .. }
            | CliError::Dialoguer(_)
            | CliError::Manifest(_)
            | CliError::Runtime(_) => None,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Usage(message) | CliError::Runtime(message) => write!(f, "{message}"),
            CliError::Core(error) => write!(f, "{error}"),
            CliError::Memory(error) => write!(f, "{error}"),
            CliError::Registry(error) => write!(f, "{error}"),
            CliError::Io { message, .. } => write!(f, "{message}"),
            CliError::Dialoguer(error) => write!(f, "{error}"),
            CliError::Manifest(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CliError::Core(error) => Some(error),
            CliError::Memory(error) => Some(error),
            CliError::Registry(error) => Some(error),
            CliError::Io { source, .. } => Some(source),
            CliError::Dialoguer(error) => Some(error),
            CliError::Manifest(error) => Some(error),
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

impl From<RegistryError> for CliError {
    fn from(value: RegistryError) -> Self {
        Self::Registry(value)
    }
}

impl From<io::Error> for CliError {
    fn from(value: io::Error) -> Self {
        Self::io(value.to_string(), value)
    }
}

impl From<dialoguer::Error> for CliError {
    fn from(value: dialoguer::Error) -> Self {
        Self::Dialoguer(value)
    }
}

impl From<craft_manifest::ManifestError> for CliError {
    fn from(value: craft_manifest::ManifestError) -> Self {
        Self::Manifest(value)
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
            ui::message(format!("craft {VERSION}"));
            Ok(())
        }
        Some("init") => {
            let target = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            init_project(&target)
        }
        Some("doctor") => {
            doctor();
            Ok(())
        }
        Some("harness") => harness_command(&args[1..]),
        Some("install") => {
            let mut harness_args = vec!["install".to_string()];
            harness_args.extend(args[1..].iter().cloned());
            harness_command(&harness_args)
        }
        Some("publish") => publish_command(&args[1..]),
        Some("compose") => compose_command(&args[1..]),
        Some("compose-plan") => compose_plan_command(&args[1..]),
        Some("run") => run_compose_command(&args[1..]),
        Some("lsp") => lsp_command(),
        Some("validate") => validate_command(&args[1..]),
        Some("login") => login_command(&args[1..]),
        Some("org") => org_command(&args[1..]),
        Some("team") => team_command(&args[1..]),
        Some("memory") => memory_command(&args[1..]),
        Some("completions") => completions_command(&args[1..]),
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
  craft publish [--registry <url>] [--org <org>]
  craft install <org>/<name>[@version]
  craft harness publish [--registry <url>] [--org <org>]
  craft harness install <org>/<name>[@version]
  craft harness install github:owner/repo[@ref]
  craft harness list
  craft harness info <name>
  craft harness test <name>
  craft harness uninstall <name>
  craft compose <harness> [harness...] [-o craft.compose.toml] [--plan] [--strategy <strategy>]
  craft compose-plan <harness> [harness...] [--strategy <strategy>]
  craft run [craft.compose.toml] --model <model> [--runtime ollama] [--prompt <text>]
  craft lsp             Start the craft.toml language server on stdio
  craft validate [path]   Validate a harness manifest and TDD checks
  craft login --api-key <key> [--registry <url>]
  craft org list
  craft org create <name> [--display-name <name>] [--description <text>] [--visibility <visibility>]
  craft org info <name>
  craft org invite <org> <email> [--role <role>]
  craft org members <org>
  craft org remove-member <org> <user-id> [--yes]
  craft org delete <org> [--yes]
  craft team list <org>
  craft team create <org> <name> [--display-name <name>] [--description <text>] [--visibility <visibility>]
  craft team info <org> <team>
  craft team members <org> <team>
  craft team add-member <org> <team> <user-id> [--role <role>]
  craft team remove-member <org> <team> <user-id> [--yes]
  craft team delete <org> <team> [--yes]
  craft memory log <scope> <key> <value>
  craft memory recall <scope> <key>
  craft memory record --scope <scope> --key <key> --value <value>
  craft memory inspect --scope <scope>
  craft memory search --query <query> [--scope <scope>...]
  craft memory context --query <query> [--tokens 1200] [--scope <scope>...]
  craft completions <shell>
  craft version          Print version
  craft --help           Print help

Shells:
  bash, zsh, fish, powershell, elvish
"
    );
}

fn completions_command(args: &[String]) -> Result<(), CliError> {
    let shell = args
        .first()
        .ok_or_else(|| CliError::usage("usage: craft completions <shell>"))?;
    let mut command = completion_command();
    match shell.as_str() {
        "bash" => generate(shells::Bash, &mut command, "craft", &mut io::stdout()),
        "zsh" => generate(shells::Zsh, &mut command, "craft", &mut io::stdout()),
        "fish" => generate(shells::Fish, &mut command, "craft", &mut io::stdout()),
        "powershell" | "ps1" => {
            generate(shells::PowerShell, &mut command, "craft", &mut io::stdout())
        }
        "elvish" => generate(shells::Elvish, &mut command, "craft", &mut io::stdout()),
        value => {
            return Err(CliError::usage(format!(
                "unknown completion shell `{value}`; use bash, zsh, fish, powershell, or elvish"
            )));
        }
    }
    Ok(())
}

fn completion_command() -> Command {
    Command::new("craft")
        .version(VERSION)
        .about("CRAFT expertise harness CLI")
        .subcommand(Command::new("init").arg(Arg::new("path")))
        .subcommand(Command::new("doctor"))
        .subcommand(
            Command::new("publish")
                .arg(Arg::new("registry").long("registry").value_name("URL"))
                .arg(Arg::new("org").long("org").value_name("ORG")),
        )
        .subcommand(Command::new("install").arg(Arg::new("source").required(true)))
        .subcommand(
            Command::new("harness")
                .subcommand(
                    Command::new("publish")
                        .arg(Arg::new("registry").long("registry").value_name("URL"))
                        .arg(Arg::new("org").long("org").value_name("ORG")),
                )
                .subcommand(Command::new("install").arg(Arg::new("source").required(true)))
                .subcommand(Command::new("list"))
                .subcommand(Command::new("info").arg(Arg::new("name").required(true)))
                .subcommand(Command::new("test").arg(Arg::new("name").required(true)))
                .subcommand(
                    Command::new("uninstall")
                        .arg(Arg::new("name").required(true))
                        .arg(
                            Arg::new("yes")
                                .short('y')
                                .long("yes")
                                .action(ArgAction::SetTrue),
                        ),
                ),
        )
        .subcommand(
            Command::new("compose")
                .arg(Arg::new("harness").num_args(1..))
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_name("PATH"),
                )
                .arg(Arg::new("plan").long("plan").action(ArgAction::SetTrue))
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue),
                )
                .arg(Arg::new("strategy").long("strategy").value_name("STRATEGY")),
        )
        .subcommand(
            Command::new("compose-plan")
                .arg(Arg::new("harness").num_args(1..))
                .arg(Arg::new("strategy").long("strategy").value_name("STRATEGY")),
        )
        .subcommand(
            Command::new("run")
                .arg(Arg::new("compose"))
                .arg(
                    Arg::new("model")
                        .long("model")
                        .required(true)
                        .value_name("MODEL"),
                )
                .arg(Arg::new("runtime").long("runtime").value_name("RUNTIME"))
                .arg(Arg::new("prompt").long("prompt").value_name("TEXT")),
        )
        .subcommand(Command::new("lsp"))
        .subcommand(Command::new("validate").arg(Arg::new("path")))
        .subcommand(
            Command::new("login")
                .arg(
                    Arg::new("api-key")
                        .long("api-key")
                        .required(true)
                        .value_name("KEY"),
                )
                .arg(Arg::new("registry").long("registry").value_name("URL")),
        )
        .subcommand(
            Command::new("org")
                .subcommand(Command::new("list"))
                .subcommand(
                    Command::new("create")
                        .arg(Arg::new("name").required(true))
                        .arg(Arg::new("display-name").long("display-name"))
                        .arg(Arg::new("description").long("description"))
                        .arg(Arg::new("visibility").long("visibility")),
                )
                .subcommand(Command::new("info").arg(Arg::new("name").required(true)))
                .subcommand(
                    Command::new("invite")
                        .arg(Arg::new("org").required(true))
                        .arg(Arg::new("email").required(true))
                        .arg(Arg::new("role").long("role")),
                )
                .subcommand(Command::new("members").arg(Arg::new("org").required(true)))
                .subcommand(
                    Command::new("remove-member")
                        .arg(Arg::new("org").required(true))
                        .arg(Arg::new("user-id").required(true))
                        .arg(
                            Arg::new("yes")
                                .long("yes")
                                .short('y')
                                .action(ArgAction::SetTrue),
                        ),
                )
                .subcommand(
                    Command::new("delete")
                        .arg(Arg::new("org").required(true))
                        .arg(
                            Arg::new("yes")
                                .long("yes")
                                .short('y')
                                .action(ArgAction::SetTrue),
                        ),
                ),
        )
        .subcommand(
            Command::new("team")
                .subcommand(Command::new("list").arg(Arg::new("org").required(true)))
                .subcommand(
                    Command::new("create")
                        .arg(Arg::new("org").required(true))
                        .arg(Arg::new("name").required(true))
                        .arg(Arg::new("display-name").long("display-name"))
                        .arg(Arg::new("description").long("description"))
                        .arg(Arg::new("visibility").long("visibility")),
                )
                .subcommand(
                    Command::new("info")
                        .arg(Arg::new("org").required(true))
                        .arg(Arg::new("team").required(true)),
                )
                .subcommand(
                    Command::new("members")
                        .arg(Arg::new("org").required(true))
                        .arg(Arg::new("team").required(true)),
                )
                .subcommand(
                    Command::new("add-member")
                        .arg(Arg::new("org").required(true))
                        .arg(Arg::new("team").required(true))
                        .arg(Arg::new("user-id").required(true))
                        .arg(Arg::new("role").long("role")),
                )
                .subcommand(
                    Command::new("remove-member")
                        .arg(Arg::new("org").required(true))
                        .arg(Arg::new("team").required(true))
                        .arg(Arg::new("user-id").required(true))
                        .arg(
                            Arg::new("yes")
                                .long("yes")
                                .short('y')
                                .action(ArgAction::SetTrue),
                        ),
                )
                .subcommand(
                    Command::new("delete")
                        .arg(Arg::new("org").required(true))
                        .arg(Arg::new("team").required(true))
                        .arg(
                            Arg::new("yes")
                                .long("yes")
                                .short('y')
                                .action(ArgAction::SetTrue),
                        ),
                ),
        )
        .subcommand(
            Command::new("memory")
                .subcommand(
                    Command::new("log")
                        .arg(Arg::new("scope").required(true))
                        .arg(Arg::new("key").required(true))
                        .arg(Arg::new("value").required(true)),
                )
                .subcommand(
                    Command::new("record")
                        .arg(Arg::new("scope").long("scope").required(true))
                        .arg(Arg::new("key").long("key").required(true))
                        .arg(Arg::new("value").long("value").required(true)),
                )
                .subcommand(
                    Command::new("inspect").arg(Arg::new("scope").long("scope").required(true)),
                )
                .subcommand(
                    Command::new("recall")
                        .arg(Arg::new("scope").long("scope"))
                        .arg(Arg::new("query").long("query"))
                        .arg(Arg::new("positional").num_args(0..)),
                )
                .subcommand(
                    Command::new("search")
                        .arg(Arg::new("query").long("query").required(true))
                        .arg(Arg::new("scope").long("scope").action(ArgAction::Append)),
                )
                .subcommand(
                    Command::new("context")
                        .arg(Arg::new("query").long("query"))
                        .arg(Arg::new("tokens").long("tokens"))
                        .arg(Arg::new("scope").long("scope").action(ArgAction::Append)),
                ),
        )
        .subcommand(Command::new("completions").arg(Arg::new("shell").required(true)))
}

fn login_command(args: &[String]) -> Result<(), CliError> {
    let mut api_key = None;
    let mut registry = "http://localhost:8080".to_string();
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--api-key" => {
                api_key = iter.next().cloned();
            }
            "--registry" => {
                registry = iter.next().cloned().ok_or_else(|| {
                    CliError::usage("usage: craft login --api-key <key> [--registry <url>]")
                })?;
            }
            "-h" | "--help" => {
                ui::message("usage: craft login --api-key <key> [--registry <url>]");
                return Ok(());
            }
            value => {
                return Err(CliError::usage(format!(
                    "unknown login argument `{value}`\n\nRun `craft login --help`."
                )));
            }
        }
    }

    let api_key = api_key
        .ok_or_else(|| CliError::usage("usage: craft login --api-key <key> [--registry <url>]"))?;
    let config_path = default_registry_config_path();
    save_registry_credentials(&config_path, &registry, &api_key)?;

    ui::success(format!(
        "registry credentials saved to {}",
        config_path.display()
    ));
    Ok(())
}

fn default_registry_config_path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("craft")
        .join("registry.toml")
}

fn save_registry_credentials(
    path: &Path,
    registry_url: &str,
    api_key: &str,
) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| CliError::io(format!("failed to create {}", parent.display()), err))?;
    }

    let content = format!(
        "registry_url = \"{}\"\nauth_token = \"{}\"\n",
        toml_escape(registry_url),
        toml_escape(api_key)
    );
    fs::write(path, content)
        .map_err(|err| CliError::io(format!("failed to write {}", path.display()), err))?;
    Ok(())
}

fn org_command(args: &[String]) -> Result<(), CliError> {
    let registry = cloud_registry()?;
    match args.first().map(String::as_str) {
        Some("list") => {
            let orgs = registry::block_on(registry.list_orgs())?;
            if orgs.is_empty() {
                ui::message("no organizations found");
            } else {
                let rows: Vec<Vec<String>> = orgs
                    .into_iter()
                    .map(|org| {
                        vec![
                            org.name,
                            org.display_name.unwrap_or_default(),
                            org.visibility,
                            org.owner_id.unwrap_or_default(),
                        ]
                    })
                    .collect();
                ui::table(&["name", "display_name", "visibility", "owner"], &rows);
            }
            Ok(())
        }
        Some("create") => {
            let name = positional(
                args,
                1,
                "usage: craft org create <name> [--display-name <name>] [--description <text>] [--visibility <visibility>]",
            )?;
            let display_name = optional_flag(args, "--display-name").or_else(|| {
                if io::stdin().is_terminal() {
                    dialoguer::Input::<String>::with_theme(&ColorfulTheme::default())
                        .with_prompt("Display name")
                        .default(name.clone())
                        .interact_text()
                        .ok()
                } else {
                    None
                }
            });
            let description = optional_flag(args, "--description");
            let visibility = optional_flag(args, "--visibility");
            let spinner = ui::spinner(format!("creating organization {name}"));
            let org = registry::block_on(registry.create_org(&registry::CreateOrgRequest {
                name: &name,
                display_name: display_name.as_deref(),
                description: description.as_deref(),
                visibility: visibility.as_deref(),
            }))?;
            ui::finish_spinner(spinner, format!("created organization {}", org.name));
            ui::success(format!("created organization {}", org.name));
            Ok(())
        }
        Some("info") => {
            let name = positional(args, 1, "usage: craft org info <name>")?;
            let org = registry::block_on(registry.get_org(&name))?;
            ui::table(
                &["field", "value"],
                &[
                    vec!["name".to_string(), org.name],
                    vec![
                        "display_name".to_string(),
                        org.display_name.unwrap_or_default(),
                    ],
                    vec!["visibility".to_string(), org.visibility],
                    vec!["owner_id".to_string(), org.owner_id.unwrap_or_default()],
                    vec!["created_at".to_string(), org.created_at],
                ],
            );
            Ok(())
        }
        Some("invite") => {
            let org = positional(
                args,
                1,
                "usage: craft org invite <org> <email> [--role <role>]",
            )?;
            let email = positional(
                args,
                2,
                "usage: craft org invite <org> <email> [--role <role>]",
            )?;
            let role = optional_flag(args, "--role").unwrap_or_else(|| "member".to_string());
            let spinner = ui::spinner(format!("inviting {email} to {org}"));
            let member = registry::block_on(registry.invite_org_member(
                &org,
                &registry::InviteOrgMemberRequest {
                    email: &email,
                    role: Some(&role),
                },
            ))?;
            match &member {
                registry::InviteOrgMemberResponse::Member { user, role, .. } => {
                    ui::finish_spinner(spinner, format!("invited {}", user.email));
                    ui::success(format!("invited {} to {org} as {role}", user.email));
                }
                registry::InviteOrgMemberResponse::Invitation { email, role, .. } => {
                    ui::finish_spinner(spinner, format!("invited {}", email));
                    ui::success(format!("invited {email} to {org} as {role}"));
                }
            }
            Ok(())
        }
        Some("members") => {
            let org = positional(args, 1, "usage: craft org members <org>")?;
            let members = registry::block_on(registry.list_org_members(&org))?;
            print_members(members);
            Ok(())
        }
        Some("remove-member") => {
            let org = positional(
                args,
                1,
                "usage: craft org remove-member <org> <user-id> [--yes]",
            )?;
            let user_id = positional(
                args,
                2,
                "usage: craft org remove-member <org> <user-id> [--yes]",
            )?;
            confirm_or_cancel(
                args,
                format!("Remove user `{user_id}` from organization `{org}`?"),
            )?;
            let spinner = ui::spinner(format!("removing {user_id} from {org}"));
            registry::block_on(registry.remove_org_member(&org, &user_id))?;
            ui::finish_spinner(spinner, format!("removed {user_id}"));
            ui::success(format!("removed {user_id} from {org}"));
            Ok(())
        }
        Some("delete") => {
            let org = positional(args, 1, "usage: craft org delete <org> [--yes]")?;
            confirm_or_cancel(args, format!("Delete organization `{org}`?"))?;
            let spinner = ui::spinner(format!("deleting organization {org}"));
            registry::block_on(registry.delete_org(&org))?;
            ui::finish_spinner(spinner, format!("deleted {org}"));
            ui::success(format!("deleted organization {org}"));
            Ok(())
        }
        _ => Err(CliError::usage(
            "usage: craft org <list|create|info|invite|members|remove-member|delete>",
        )),
    }
}

fn team_command(args: &[String]) -> Result<(), CliError> {
    let registry = cloud_registry()?;
    match args.first().map(String::as_str) {
        Some("list") => {
            let org = positional(args, 1, "usage: craft team list <org>")?;
            let teams = registry::block_on(registry.list_teams(&org))?;
            if teams.is_empty() {
                ui::message("no teams found");
            } else {
                let rows: Vec<Vec<String>> = teams
                    .into_iter()
                    .map(|team| {
                        vec![
                            team.name,
                            team.display_name.unwrap_or_default(),
                            team.visibility,
                            team.description.unwrap_or_default(),
                        ]
                    })
                    .collect();
                ui::table(
                    &["name", "display_name", "visibility", "description"],
                    &rows,
                );
            }
            Ok(())
        }
        Some("create") => {
            let org = positional(
                args,
                1,
                "usage: craft team create <org> <name> [--display-name <name>] [--description <text>] [--visibility <visibility>]",
            )?;
            let name = positional(
                args,
                2,
                "usage: craft team create <org> <name> [--display-name <name>] [--description <text>] [--visibility <visibility>]",
            )?;
            let display_name = optional_flag(args, "--display-name").or_else(|| {
                if io::stdin().is_terminal() {
                    dialoguer::Input::<String>::with_theme(&ColorfulTheme::default())
                        .with_prompt("Display name")
                        .default(name.clone())
                        .interact_text()
                        .ok()
                } else {
                    None
                }
            });
            let description = optional_flag(args, "--description");
            let visibility = optional_flag(args, "--visibility");
            let spinner = ui::spinner(format!("creating team {org}/{name}"));
            let team = registry::block_on(registry.create_team(
                &org,
                &registry::CreateTeamRequest {
                    name: &name,
                    display_name: display_name.as_deref(),
                    description: description.as_deref(),
                    visibility: visibility.as_deref(),
                },
            ))?;
            ui::finish_spinner(spinner, format!("created team {}/{}", team.org, team.name));
            ui::success(format!("created team {}/{}", team.org, team.name));
            Ok(())
        }
        Some("info") => {
            let org = positional(args, 1, "usage: craft team info <org> <team>")?;
            let team_name = positional(args, 2, "usage: craft team info <org> <team>")?;
            let team = registry::block_on(registry.get_team(&org, &team_name))?;
            ui::table(
                &["field", "value"],
                &[
                    vec!["org".to_string(), team.org],
                    vec!["name".to_string(), team.name],
                    vec![
                        "display_name".to_string(),
                        team.display_name.unwrap_or_default(),
                    ],
                    vec!["visibility".to_string(), team.visibility],
                    vec![
                        "description".to_string(),
                        team.description.unwrap_or_default(),
                    ],
                    vec!["created_at".to_string(), team.created_at],
                ],
            );
            Ok(())
        }
        Some("members") => {
            let org = positional(args, 1, "usage: craft team members <org> <team>")?;
            let team = positional(args, 2, "usage: craft team members <org> <team>")?;
            let members = registry::block_on(registry.list_team_members(&org, &team))?;
            print_members(members);
            Ok(())
        }
        Some("add-member") => {
            let org = positional(
                args,
                1,
                "usage: craft team add-member <org> <team> <user-id> [--role <role>]",
            )?;
            let team = positional(
                args,
                2,
                "usage: craft team add-member <org> <team> <user-id> [--role <role>]",
            )?;
            let user_id = positional(
                args,
                3,
                "usage: craft team add-member <org> <team> <user-id> [--role <role>]",
            )?;
            let role = optional_flag(args, "--role").unwrap_or_else(|| "member".to_string());
            let spinner = ui::spinner(format!("adding {user_id} to {org}/{team}"));
            let member = registry::block_on(registry.invite_team_member(
                &org,
                &team,
                &registry::InviteTeamMemberRequest {
                    user_id: &user_id,
                    role: Some(&role),
                },
            ))?;
            ui::finish_spinner(spinner, format!("added {}", member.user.username));
            ui::success(format!(
                "added {} to {org}/{team} as {}",
                member.user.username, member.role
            ));
            Ok(())
        }
        Some("remove-member") => {
            let org = positional(
                args,
                1,
                "usage: craft team remove-member <org> <team> <user-id> [--yes]",
            )?;
            let team = positional(
                args,
                2,
                "usage: craft team remove-member <org> <team> <user-id> [--yes]",
            )?;
            let user_id = positional(
                args,
                3,
                "usage: craft team remove-member <org> <team> <user-id> [--yes]",
            )?;
            confirm_or_cancel(
                args,
                format!("Remove user `{user_id}` from team `{org}/{team}`?"),
            )?;
            let spinner = ui::spinner(format!("removing {user_id} from {org}/{team}"));
            registry::block_on(registry.remove_team_member(&org, &team, &user_id))?;
            ui::finish_spinner(spinner, format!("removed {user_id}"));
            ui::success(format!("removed {user_id} from {org}/{team}"));
            Ok(())
        }
        Some("delete") => {
            let org = positional(args, 1, "usage: craft team delete <org> <team> [--yes]")?;
            let team = positional(args, 2, "usage: craft team delete <org> <team> [--yes]")?;
            confirm_or_cancel(args, format!("Delete team `{org}/{team}`?"))?;
            let spinner = ui::spinner(format!("deleting team {org}/{team}"));
            registry::block_on(registry.delete_team(&org, &team))?;
            ui::finish_spinner(spinner, format!("deleted {org}/{team}"));
            ui::success(format!("deleted team {org}/{team}"));
            Ok(())
        }
        _ => Err(CliError::usage(
            "usage: craft team <list|create|info|members|add-member|remove-member|delete>",
        )),
    }
}

fn cloud_registry() -> Result<registry::CloudRegistry, CliError> {
    cloud_registry_with_override(None)
}

fn cloud_registry_with_override(
    registry_url: Option<&str>,
) -> Result<registry::CloudRegistry, CliError> {
    let path = default_registry_config_path();
    if !path.exists() {
        return Err(CliError::Runtime(format!(
            "registry credentials not found at {}; run `craft login --api-key <key> [--registry <url>]` first",
            path.display()
        )));
    }
    Ok(registry::CloudRegistry::from_config_file_with_registry(
        &path,
        registry_url,
    )?)
}

fn confirm_or_cancel(args: &[String], prompt: String) -> Result<(), CliError> {
    let confirmed = args.iter().any(|value| value == "--yes" || value == "-y")
        || (!io::stdin().is_terminal()
            || Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(prompt)
                .default(false)
                .interact()
                .map_err(CliError::from)?);
    if confirmed {
        Ok(())
    } else {
        Err(CliError::Runtime("operation cancelled".to_string()))
    }
}

fn print_members(members: Vec<registry::MemberResponse>) {
    if members.is_empty() {
        ui::message("no members found");
    } else {
        let rows: Vec<Vec<String>> = members
            .into_iter()
            .map(|member| {
                vec![
                    member.user.id,
                    member.user.username,
                    member.user.email,
                    member.role,
                    member.joined_at,
                ]
            })
            .collect();
        ui::table(&["id", "username", "email", "role", "joined_at"], &rows);
    }
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn suggest_command(command: &str) -> Option<&'static str> {
    const COMMANDS: &[&str] = &[
        "init",
        "doctor",
        "publish",
        "install",
        "harness",
        "compose",
        "compose-plan",
        "run",
        "lsp",
        "validate",
        "login",
        "memory",
        "org",
        "team",
        "completions",
        "version",
    ];
    COMMANDS
        .iter()
        .copied()
        .filter_map(|candidate| {
            let distance = edit_distance(command, candidate);
            (distance <= 3).then_some((candidate, distance))
        })
        .min_by_key(|(_, distance)| *distance)
        .map(|(candidate, _)| candidate)
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_char) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right.chars().enumerate() {
            let insertion = current[right_index] + 1;
            let deletion = previous[right_index + 1] + 1;
            let substitution = previous[right_index] + usize::from(left_char != right_char);
            current[right_index + 1] = insertion.min(deletion).min(substitution);
        }
        previous.clone_from(&current);
    }
    previous[right.len()]
}

fn init_project(root: &Path) -> Result<(), CliError> {
    create_project_dir(root.join(".craft"))?;
    create_project_dir(root.join("prompts"))?;
    create_project_dir(root.join("memory"))?;
    create_project_dir(root.join("tools"))?;
    create_project_dir(root.join("validators"))?;

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

    ui::success(format!("initialized CRAFT project at {}", root.display()));
    Ok(())
}

fn create_project_dir(path: impl AsRef<Path>) -> Result<(), CliError> {
    let path = path.as_ref();
    fs::create_dir_all(path)
        .map_err(|err| CliError::io(format!("failed to create {}", path.display()), err))
}

fn write_if_missing(path: impl AsRef<Path>, contents: &str) -> Result<(), CliError> {
    let path = path.as_ref();
    if !path.exists() {
        fs::write(path, contents)
            .map_err(|err| CliError::io(format!("failed to write {}", path.display()), err))?;
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

fn publish_command(args: &[String]) -> Result<(), CliError> {
    if args.iter().any(|value| value == "-h" || value == "--help") {
        ui::message("usage: craft publish [--registry <url>] [--org <org>]");
        return Ok(());
    }

    let registry_url = optional_flag(args, "--registry");
    let registry = cloud_registry_with_override(registry_url.as_deref())?;
    let org = optional_flag(args, "--org")
        .or_else(|| env::var("CRAFT_ORG").ok())
        .or_else(|| registry.default_org().map(str::to_string))
        .ok_or_else(|| {
            CliError::usage(
                "usage: craft publish [--registry <url>] --org <org>\n\nSet CRAFT_ORG or default_org in registry.toml to omit --org.",
            )
        })?;

    let root = PathBuf::from(".");
    let spinner = ui::spinner("validating harness");
    validate_harness_project(&root)?;
    let project = HarnessProject::load(&root)?;
    let manifest = project.manifest();
    ui::finish_spinner(spinner, format!("validated {}", manifest.harness.name));

    let package = package_harness(project.root())?;
    let package_sha256 = sha256_hex(&package);
    let name = manifest.harness.name.clone();
    let version = manifest.harness.version.clone();
    let description = manifest.harness.description.clone();

    let spinner = ui::spinner(format!("publishing {org}/{name}@{version}"));
    let published = registry::block_on(registry.publish_package(
        &org,
        &name,
        &version,
        Some(&description),
        package,
    ))?;
    if published.content_sha256 != package_sha256 {
        return Err(CliError::Runtime(format!(
            "registry checksum mismatch: local {package_sha256}, remote {}",
            published.content_sha256
        )));
    }
    ui::finish_spinner(spinner, format!("published {org}/{name}@{version}"));
    ui::success(format!("published {org}/{name}@{version}"));
    Ok(())
}

fn harness_command(args: &[String]) -> Result<(), CliError> {
    let manager = HarnessManager::new(CraftHome::from_env()?);
    match args.first().map(String::as_str) {
        Some("publish") => publish_command(&args[1..]),
        Some("install") => {
            let source = args.get(1).ok_or_else(|| {
                CliError::usage(
                    "usage: craft harness install <org>/<name>[@version]|github:owner/repo[@ref]",
                )
            })?;
            if source.starts_with("github:") {
                install_github_harness(&manager, source)
            } else if let Some(path) = source.strip_prefix("local:") {
                install_local_harness(&manager, Path::new(path))
            } else {
                install_registry_harness(&manager, source)
            }
        }
        Some("list") => {
            let registry = manager.registry()?;
            let harnesses = registry.list()?;
            if harnesses.is_empty() {
                println!("no harnesses installed");
            } else {
                let rows: Vec<Vec<String>> = harnesses
                    .into_iter()
                    .map(|harness| {
                        vec![
                            harness.name,
                            harness.version,
                            harness.source,
                            harness.path.display().to_string(),
                        ]
                    })
                    .collect();
                ui::table(&["name", "version", "source", "path"], &rows);
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
            let spinner = ui::spinner(format!("testing harness {name}"));
            let result = test_installed_harness(&registry, name)?;
            ui::finish_spinner(spinner, format!("validated {}", result.harness_name));
            print_validation_result(&result);
            Ok(())
        }
        Some("uninstall") => {
            let name = args
                .iter()
                .skip(1)
                .find(|value| !value.starts_with('-'))
                .ok_or_else(|| CliError::usage("usage: craft harness uninstall <name> [--yes]"))?;
            let confirmed = args.iter().any(|value| value == "--yes" || value == "-y")
                || !io::stdin().is_terminal()
                || Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!("Uninstall harness `{name}`?"))
                    .default(false)
                    .interact()
                    .map_err(CliError::from)?;
            if !confirmed {
                return Err(CliError::Runtime("uninstall cancelled".to_string()));
            }
            let registry = manager.registry()?;
            let harness = registry.uninstall(name, true)?;
            ui::success(format!("uninstalled {}", harness.name));
            Ok(())
        }
        _ => Err(CliError::usage(
            "usage: craft harness <publish|install|list|info|test|uninstall>",
        )),
    }
}

fn install_github_harness(manager: &HarnessManager, source: &str) -> Result<(), CliError> {
    let source = GithubSource::parse(source)?;
    let spinner = ui::spinner(format!(
        "installing harness from github:{}/{}",
        source.owner, source.repo
    ));
    let result = manager.install_github(&source)?;
    ui::finish_spinner(spinner, format!("installed {}", result.harness.name));
    ui::success(format!(
        "installed {} {} from {}",
        result.harness.name, result.harness.version, result.harness.source
    ));
    Ok(())
}

fn install_local_harness(manager: &HarnessManager, path: &Path) -> Result<(), CliError> {
    let path = fs::canonicalize(path)
        .map_err(|err| CliError::io(format!("failed to resolve {}", path.display()), err))?;
    let spinner = ui::spinner(format!("validating local harness {}", path.display()));
    validate_harness_project(&path)?;
    let project = HarnessProject::load(&path)?;
    let manifest = project.manifest();
    let installed = InstalledHarness {
        name: manifest.harness.name.clone(),
        version: manifest.harness.version.clone(),
        source: format!("local:{}", path.display()),
        path,
    };
    manager.registry()?.upsert(&installed)?;
    ui::finish_spinner(spinner, format!("installed {}", installed.name));
    ui::success(format!(
        "installed {} {} from {}",
        installed.name, installed.version, installed.source
    ));
    Ok(())
}

fn install_registry_harness(manager: &HarnessManager, source: &str) -> Result<(), CliError> {
    let (org, name, requirement) = parse_registry_source(source)?;
    let registry = cloud_registry()?;
    let spinner = ui::spinner(format!("resolving {org}/{name}"));
    let version =
        registry::block_on(registry.resolve_version(&org, &name, requirement.as_deref()))?;
    ui::finish_spinner(
        spinner,
        format!("resolved {org}/{name}@{}", version.version),
    );

    let spinner = ui::spinner(format!("downloading {org}/{name}@{}", version.version));
    let package = registry::block_on(registry.download_harness(&org, &name, &version.version))?;
    let package_sha256 = sha256_hex(&package);
    if package_sha256 != version.content_sha256 {
        return Err(CliError::Runtime(format!(
            "download checksum mismatch: expected {}, got {package_sha256}",
            version.content_sha256
        )));
    }

    let home = CraftHome::from_env()?;
    home.ensure()?;
    let target = home
        .harnesses_dir()
        .join(&org)
        .join(&name)
        .join(&version.version);
    if target.exists() {
        return Err(CliError::Runtime(format!(
            "target harness directory already exists: {}",
            target.display()
        )));
    }
    let parent = target
        .parent()
        .ok_or_else(|| CliError::Runtime("invalid harness target path".to_string()))?;
    fs::create_dir_all(parent)
        .map_err(|err| CliError::io(format!("failed to create {}", parent.display()), err))?;

    let temp = tempfile::Builder::new()
        .prefix("craft-install-")
        .tempdir_in(parent)
        .map_err(|err| {
            CliError::io(
                format!("failed to create temp dir in {}", parent.display()),
                err,
            )
        })?;
    unpack_harness_archive(&package, temp.path())?;
    validate_harness_project(temp.path())?;
    let temp_path = temp.path().to_path_buf();
    fs::rename(&temp_path, &target).map_err(|err| {
        CliError::io(
            format!("failed to move harness into {}", target.display()),
            err,
        )
    })?;

    let installed = InstalledHarness {
        name: name.clone(),
        version: version.version.clone(),
        source: format!("registry:{org}/{name}@{}", version.version),
        path: target,
    };
    manager.registry()?.upsert(&installed)?;
    update_registry_lockfile(
        Path::new(LOCKFILE_NAME),
        LockedHarness {
            org: org.clone(),
            name: name.clone(),
            version: version.version.clone(),
            source: format!("registry:{org}/{name}"),
            checksum: version.content_sha256.clone(),
        },
    )?;
    ui::finish_spinner(spinner, format!("installed {name} {}", version.version));
    ui::success(format!(
        "installed {} {} from {}",
        installed.name, installed.version, installed.source
    ));
    Ok(())
}

fn parse_registry_source(source: &str) -> Result<(String, String, Option<String>), CliError> {
    let raw = source.strip_prefix("registry:").unwrap_or(source);
    let (path, version) = match raw.rsplit_once('@') {
        Some((path, version)) if !version.trim().is_empty() => {
            (path, Some(version.trim().to_string()))
        }
        Some(_) => {
            return Err(CliError::usage(
                "registry source version requirement must not be empty",
            ));
        }
        None => (raw, None),
    };
    let (org, name) = path
        .split_once('/')
        .ok_or_else(|| CliError::usage("usage: craft harness install <org>/<name>[@version]"))?;
    validate_registry_slug("org", org)?;
    validate_registry_slug("name", name)?;
    Ok((org.to_string(), name.to_string(), version))
}

fn validate_registry_slug(label: &str, value: &str) -> Result<(), CliError> {
    let valid = !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        });
    if valid {
        Ok(())
    } else {
        Err(CliError::usage(format!(
            "invalid {label} `{value}`; use letters, numbers, hyphen, or underscore"
        )))
    }
}

fn package_harness(root: &Path) -> Result<Vec<u8>, CliError> {
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = Builder::new(encoder);
    append_harness_dir(&mut builder, root, root)?;
    let encoder = builder
        .into_inner()
        .map_err(|err| CliError::io("failed to finish tar archive", err))?;
    encoder
        .finish()
        .map_err(|err| CliError::io("failed to finish gzip archive", err))
}

fn append_harness_dir(
    builder: &mut Builder<GzEncoder<Vec<u8>>>,
    root: &Path,
    dir: &Path,
) -> Result<(), CliError> {
    for entry in fs::read_dir(dir)
        .map_err(|err| CliError::io(format!("failed to read {}", dir.display()), err))?
    {
        let entry = entry.map_err(|err| CliError::io("failed to read directory entry", err))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| CliError::Runtime(format!("failed to package {}", path.display())))?;
        if should_skip_package_path(relative) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|err| CliError::io(format!("failed to inspect {}", path.display()), err))?;
        if file_type.is_dir() {
            builder
                .append_dir(relative, &path)
                .map_err(|err| CliError::io(format!("failed to add {}", path.display()), err))?;
            append_harness_dir(builder, root, &path)?;
        } else if file_type.is_file() {
            builder
                .append_path_with_name(&path, relative)
                .map_err(|err| CliError::io(format!("failed to add {}", path.display()), err))?;
        }
    }
    Ok(())
}

fn should_skip_package_path(relative: &Path) -> bool {
    relative.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        matches!(value.as_ref(), ".git" | "target" | ".DS_Store")
    })
}

fn unpack_harness_archive(package: &[u8], target: &Path) -> Result<(), CliError> {
    let decoder = GzDecoder::new(package);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(target)
        .map_err(|err| CliError::io(format!("failed to extract into {}", target.display()), err))
}

fn update_registry_lockfile(path: &Path, locked: LockedHarness) -> Result<(), CliError> {
    let mut lockfile = if path.exists() {
        let contents = fs::read_to_string(path)
            .map_err(|err| CliError::io(format!("failed to read {}", path.display()), err))?;
        toml::from_str::<HarnessLockfile>(&contents)
            .map_err(|err| CliError::Runtime(format!("invalid {}: {err}", path.display())))?
    } else {
        HarnessLockfile {
            version: LOCKFILE_VERSION,
            harness: Vec::new(),
        }
    };

    if lockfile.version != LOCKFILE_VERSION {
        return Err(CliError::Runtime(format!(
            "unsupported {} format version {}; expected {LOCKFILE_VERSION}",
            path.display(),
            lockfile.version
        )));
    }

    if let Some(existing) = lockfile
        .harness
        .iter_mut()
        .find(|entry| entry.org == locked.org && entry.name == locked.name)
    {
        *existing = locked;
    } else {
        lockfile.harness.push(locked);
    }
    lockfile
        .harness
        .sort_by(|left, right| (&left.org, &left.name).cmp(&(&right.org, &right.name)));

    let contents = toml::to_string_pretty(&lockfile).map_err(|err| {
        CliError::Runtime(format!("failed to serialize {}: {err}", path.display()))
    })?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temp = tempfile::NamedTempFile::new_in(parent).map_err(|err| {
        CliError::io(
            format!(
                "failed to create temporary lockfile in {}",
                parent.display()
            ),
            err,
        )
    })?;
    fs::write(temp.path(), contents)
        .map_err(|err| CliError::io("failed to write temporary lockfile", err))?;
    temp.persist(path)
        .map_err(|err| CliError::io(format!("failed to replace {}", path.display()), err.error))?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn validate_command(args: &[String]) -> Result<(), CliError> {
    let root = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let spinner = ui::spinner(format!("validating {}", root.display()));
    let result = validate_harness_project(root)?;
    ui::finish_spinner(spinner, format!("validated {}", result.harness_name));
    print_validation_result(&result);
    Ok(())
}

fn print_validation_result(result: &ValidationResult) {
    ui::success(format!("validated {}", result.harness_name));
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
            "usage: craft compose <harness> [harness...] [-o craft.compose.toml] [--plan] [--strategy <strategy>]",
        ));
    }

    let mut names = Vec::new();
    let mut output = PathBuf::from("craft.compose.toml");
    let mut show_plan = false;
    let mut strategy = ConflictStrategy::default();
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
            "--strategy" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| CliError::usage("craft compose --strategy requires a value"))?;
                strategy = ConflictStrategy::from_string(value).ok_or_else(|| {
                    CliError::usage(format!(
                        "unknown strategy `{value}`; use ordered-merge, merge, override, or fail"
                    ))
                })?;
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
        let spinner = ui::spinner("planning composition");
        let plan = plan_composition(&registry, &names, strategy)?;
        ui::finish_spinner(spinner, "composition plan ready");
        print_composition_plan(&plan);
        return Ok(());
    }

    let spinner = ui::spinner("composing harnesses");
    let result = compose_harnesses(&registry, &names, &output, strategy)?;
    ui::finish_spinner(spinner, format!("wrote {}", result.output_path.display()));
    for warning in result.warnings {
        ui::warning(warning);
    }
    ui::success(format!("wrote {}", result.output_path.display()));
    Ok(())
}

fn compose_plan_command(args: &[String]) -> Result<(), CliError> {
    if args.is_empty() {
        return Err(CliError::usage(
            "usage: craft compose-plan <harness> [harness...] [--strategy <strategy>]",
        ));
    }

    let mut names = Vec::new();
    let mut strategy = ConflictStrategy::default();
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
            "--strategy" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    CliError::usage("craft compose-plan --strategy requires a value")
                })?;
                strategy = ConflictStrategy::from_string(value).ok_or_else(|| {
                    CliError::usage(format!(
                        "unknown strategy `{value}`; use ordered-merge, merge, override, or fail"
                    ))
                })?;
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
    let spinner = ui::spinner("planning composition");
    let plan = plan_composition(&registry, &names, strategy)?;
    ui::finish_spinner(spinner, "composition plan ready");
    print_composition_plan(&plan);
    Ok(())
}

fn print_composition_plan(plan: &craft_core::CompositionPlan) {
    println!("composition plan");
    println!("strategy: {}", plan.strategy.as_str());
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
    match plan.strategy {
        ConflictStrategy::OrderedMerge | ConflictStrategy::Merge => {
            println!("- memory.schemas: namespaced by harness name");
            println!("- tools.mcp: namespaced by harness name");
            println!("- validators.tdd: namespaced by harness name");
        }
        ConflictStrategy::Override => {
            println!("- memory.schemas: last harness wins");
            println!("- tools.mcp: last harness wins");
            println!("- validators.tdd: last harness wins");
        }
        ConflictStrategy::Fail => {
            println!("- memory.schemas: conflict detection enabled");
            println!("- tools.mcp: conflict detection enabled");
            println!("- validators.tdd: conflict detection enabled");
        }
    }
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

    let spinner = ui::spinner(format!("running {model} with {runtime}"));
    let status = ProcessCommand::new(&runtime)
        .arg("run")
        .arg(&model)
        .arg(prompt)
        .status()
        .map_err(|err| CliError::io(format!("failed to run {runtime}: {err}"), err))?;
    if status.success() {
        ui::finish_spinner(spinner, format!("{runtime} completed"));
        Ok(())
    } else {
        spinner.finish_and_clear();
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
    match ProcessCommand::new(binary).arg(arg).output() {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn locked(org: &str, name: &str, version: &str) -> LockedHarness {
        LockedHarness {
            org: org.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            source: format!("registry:{org}/{name}"),
            checksum: format!("checksum-{version}"),
        }
    }

    #[test]
    fn registry_source_parses_latest_exact_and_semver_requirements() {
        let latest = parse_registry_source("acme/designer").unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(latest, ("acme".to_string(), "designer".to_string(), None));

        let exact = parse_registry_source("registry:acme/designer@1.2.3")
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(
            exact,
            (
                "acme".to_string(),
                "designer".to_string(),
                Some("1.2.3".to_string())
            )
        );

        let requirement =
            parse_registry_source("acme/designer@^1.2").unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(requirement.2, Some("^1.2".to_string()));
    }

    #[test]
    fn registry_lockfile_is_sorted_and_replaces_a_resolved_harness() {
        let directory = tempfile::tempdir().unwrap_or_else(|err| panic!("{err}"));
        let path = directory.path().join(LOCKFILE_NAME);

        update_registry_lockfile(&path, locked("zeta", "designer", "1.0.0"))
            .unwrap_or_else(|err| panic!("{err}"));
        update_registry_lockfile(&path, locked("acme", "architect", "2.0.0"))
            .unwrap_or_else(|err| panic!("{err}"));
        update_registry_lockfile(&path, locked("zeta", "designer", "1.1.0"))
            .unwrap_or_else(|err| panic!("{err}"));

        let contents = fs::read_to_string(&path).unwrap_or_else(|err| panic!("{err}"));
        let parsed: HarnessLockfile =
            toml::from_str(&contents).unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(parsed.version, LOCKFILE_VERSION);
        assert_eq!(parsed.harness.len(), 2);
        assert_eq!(parsed.harness[0].org, "acme");
        assert_eq!(parsed.harness[1].version, "1.1.0");
        assert_eq!(parsed.harness[1].checksum, "checksum-1.1.0");
    }
}
