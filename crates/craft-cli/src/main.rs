use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitCode};

use clap::{Arg, ArgAction, Command};
use clap_complete::{generate, shells};
use craft_core::{
    ConflictStrategy, CraftError, CraftHome, GithubSource, HarnessManager, ValidationResult,
    compose_harnesses, plan_composition, test_installed_harness, validate_harness_project,
};
use craft_memory::{Memory, MemoryError, MemoryScope};
use craft_registry::RegistryError;
use dialoguer::{Confirm, theme::ColorfulTheme};

mod registry;
mod ui;

const VERSION: &str = env!("CARGO_PKG_VERSION");

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
            CliError::Io { .. } | CliError::Runtime(_) => None,
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
  craft team create <org> <name> [--description <text>] [--visibility <visibility>]
  craft team info <org> <team>
  craft team members <org> <team>
  craft team add-member <org> <team> <username> [--role <role>]
  craft team remove-member <org> <team> <username> [--yes]
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
            Command::new("harness")
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
                        .arg(Arg::new("username").required(true))
                        .arg(Arg::new("role").long("role")),
                )
                .subcommand(
                    Command::new("remove-member")
                        .arg(Arg::new("org").required(true))
                        .arg(Arg::new("team").required(true))
                        .arg(Arg::new("username").required(true))
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

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn cloud_registry() -> Result<registry::CloudRegistry, CliError> {
    let path = default_registry_config_path();
    registry::CloudRegistry::from_config_file(&path).map_err(|err| match err {
        RegistryError::Io(source) if source.kind() == io::ErrorKind::NotFound => {
            CliError::usage(format!(
                "registry credentials not found at {}; run `craft login --api-key <key> [--registry <url>]`",
                path.display()
            ))
        }
        other => CliError::Registry(other),
    })
}

fn org_command(args: &[String]) -> Result<(), CliError> {
    let registry = cloud_registry()?;
    match args.first().map(String::as_str) {
        Some("list") => {
            let orgs = registry::block_on(registry.list_orgs())?;
            if orgs.is_empty() {
                println!("no organizations found");
            } else {
                let rows = orgs
                    .into_iter()
                    .map(|org| {
                        vec![
                            org.name,
                            org.display_name.unwrap_or_default(),
                            org.visibility,
                            org.created_at,
                        ]
                    })
                    .collect::<Vec<_>>();
                ui::table(&["name", "display", "visibility", "created"], &rows);
            }
            Ok(())
        }
        Some("create") => {
            let name = positional(args, 1, "usage: craft org create <name> [options]")?;
            let display_name = optional_flag(args, "--display-name");
            let description = optional_flag(args, "--description");
            let visibility = optional_visibility(args)?;
            let request = registry::CreateOrgRequest {
                name: &name,
                display_name: display_name.as_deref(),
                description: description.as_deref(),
                visibility: visibility.as_deref(),
            };
            let org = registry::block_on(registry.create_org(&request))?;
            ui::success(format!("created organization {}", org.name));
            print_org(&org);
            Ok(())
        }
        Some("info") => {
            let name = positional(args, 1, "usage: craft org info <name>")?;
            let org = registry::block_on(registry.get_org(&name))?;
            print_org(&org);
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
            let role = optional_role(args)?;
            let request = registry::InviteOrgMemberRequest {
                email: &email,
                role: role.as_deref(),
            };
            let member = registry::block_on(registry.invite_org_member(&org, &request))?;
            ui::success(format!(
                "added {} to {} as {}",
                member.user.email, org, member.role
            ));
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
            confirm_destructive(args, format!("Remove user `{user_id}` from `{org}`?"))?;
            registry::block_on(registry.remove_org_member(&org, &user_id))?;
            ui::success(format!("removed user {user_id} from {org}"));
            Ok(())
        }
        Some("delete") => {
            let org = positional(args, 1, "usage: craft org delete <org> [--yes]")?;
            confirm_destructive(args, format!("Delete organization `{org}`?"))?;
            registry::block_on(registry.delete_org(&org))?;
            ui::success(format!("deleted organization {org}"));
            Ok(())
        }
        Some("-h" | "--help") | None => {
            ui::message("usage: craft org <list|create|info|invite|members|remove-member|delete>");
            Ok(())
        }
        Some(command) => Err(CliError::usage(format!(
            "unknown org command `{command}`\n\nRun `craft org --help`."
        ))),
    }
}

fn team_command(args: &[String]) -> Result<(), CliError> {
    let registry = cloud_registry()?;
    match args.first().map(String::as_str) {
        Some("list") => {
            let org = positional(args, 1, "usage: craft team list <org>")?;
            let teams = registry::block_on(registry.list_teams(&org))?;
            if teams.is_empty() {
                println!("no teams found");
            } else {
                let rows = teams
                    .into_iter()
                    .map(|team| {
                        vec![
                            team.name,
                            team.description.unwrap_or_default(),
                            team.visibility,
                            team.created_at,
                        ]
                    })
                    .collect::<Vec<_>>();
                ui::table(&["name", "description", "visibility", "created"], &rows);
            }
            Ok(())
        }
        Some("create") => {
            let org = positional(args, 1, "usage: craft team create <org> <name> [options]")?;
            let name = positional(args, 2, "usage: craft team create <org> <name> [options]")?;
            let description = optional_flag(args, "--description");
            let visibility = optional_visibility(args)?;
            let request = registry::CreateTeamRequest {
                name: &name,
                description: description.as_deref(),
                visibility: visibility.as_deref(),
            };
            let team = registry::block_on(registry.create_team(&org, &request))?;
            ui::success(format!("created team {}/{}", team.org, team.name));
            print_team(&team);
            Ok(())
        }
        Some("info") => {
            let org = positional(args, 1, "usage: craft team info <org> <team>")?;
            let team = positional(args, 2, "usage: craft team info <org> <team>")?;
            let team = registry::block_on(registry.get_team(&org, &team))?;
            print_team(&team);
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
                "usage: craft team add-member <org> <team> <username> [--role <role>]",
            )?;
            let team = positional(
                args,
                2,
                "usage: craft team add-member <org> <team> <username> [--role <role>]",
            )?;
            let username = positional(
                args,
                3,
                "usage: craft team add-member <org> <team> <username> [--role <role>]",
            )?;
            let role = optional_role(args)?;
            let request = registry::InviteTeamMemberRequest {
                username: &username,
                role: role.as_deref(),
            };
            let member = registry::block_on(registry.invite_team_member(&org, &team, &request))?;
            ui::success(format!(
                "added {} to {}/{} as {}",
                member.user.username, org, team, member.role
            ));
            Ok(())
        }
        Some("remove-member") => {
            let org = positional(
                args,
                1,
                "usage: craft team remove-member <org> <team> <username> [--yes]",
            )?;
            let team = positional(
                args,
                2,
                "usage: craft team remove-member <org> <team> <username> [--yes]",
            )?;
            let username = positional(
                args,
                3,
                "usage: craft team remove-member <org> <team> <username> [--yes]",
            )?;
            confirm_destructive(args, format!("Remove `{username}` from `{org}/{team}`?"))?;
            registry::block_on(registry.remove_team_member(&org, &team, &username))?;
            ui::success(format!("removed {username} from {org}/{team}"));
            Ok(())
        }
        Some("delete") => {
            let org = positional(args, 1, "usage: craft team delete <org> <team> [--yes]")?;
            let team = positional(args, 2, "usage: craft team delete <org> <team> [--yes]")?;
            confirm_destructive(args, format!("Delete team `{org}/{team}`?"))?;
            registry::block_on(registry.delete_team(&org, &team))?;
            ui::success(format!("deleted team {org}/{team}"));
            Ok(())
        }
        Some("-h" | "--help") | None => {
            ui::message(
                "usage: craft team <list|create|info|members|add-member|remove-member|delete>",
            );
            Ok(())
        }
        Some(command) => Err(CliError::usage(format!(
            "unknown team command `{command}`\n\nRun `craft team --help`."
        ))),
    }
}

fn optional_visibility(args: &[String]) -> Result<Option<String>, CliError> {
    optional_enum(args, "--visibility", &["public", "internal", "private"])
}

fn optional_role(args: &[String]) -> Result<Option<String>, CliError> {
    optional_enum(args, "--role", &["owner", "admin", "maintainer", "member"])
}

fn optional_enum(
    args: &[String],
    flag: &str,
    allowed: &'static [&'static str],
) -> Result<Option<String>, CliError> {
    let Some(value) = optional_flag(args, flag) else {
        return Ok(None);
    };
    if allowed.contains(&value.as_str()) {
        Ok(Some(value))
    } else {
        Err(CliError::usage(format!(
            "{flag} must be one of {}",
            allowed.join(", ")
        )))
    }
}

fn confirm_destructive(args: &[String], prompt: String) -> Result<(), CliError> {
    let confirmed = args.iter().any(|value| value == "--yes" || value == "-y")
        || !io::stdin().is_terminal()
        || Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .default(false)
            .interact()
            .map_err(|err| CliError::io("failed to read confirmation", err))?;
    if confirmed {
        Ok(())
    } else {
        Err(CliError::Runtime("operation cancelled".to_string()))
    }
}

fn print_org(org: &registry::OrgResponse) {
    println!("id: {}", org.id);
    println!("name: {}", org.name);
    if let Some(display_name) = &org.display_name {
        println!("display: {display_name}");
    }
    if let Some(description) = &org.description {
        println!("description: {description}");
    }
    if let Some(owner_id) = &org.owner_id {
        println!("owner: {owner_id}");
    }
    println!("visibility: {}", org.visibility);
    println!("created: {}", org.created_at);
}

fn print_team(team: &registry::TeamResponse) {
    println!("id: {}", team.id);
    println!("org: {}", team.org);
    println!("name: {}", team.name);
    if let Some(description) = &team.description {
        println!("description: {description}");
    }
    println!("visibility: {}", team.visibility);
    println!("created: {}", team.created_at);
}

fn print_members(members: Vec<registry::MemberResponse>) {
    if members.is_empty() {
        println!("no members found");
    } else {
        let rows = members
            .into_iter()
            .map(|member| {
                vec![
                    member.user.id,
                    member.user.username,
                    member.user.email,
                    member.user.display_name.unwrap_or_default(),
                    member.user.is_admin.to_string(),
                    member.role,
                    member.joined_at,
                ]
            })
            .collect::<Vec<_>>();
        ui::table(
            &[
                "id", "username", "email", "display", "admin", "role", "joined",
            ],
            &rows,
        );
    }
}

fn suggest_command(command: &str) -> Option<&'static str> {
    const COMMANDS: &[&str] = &[
        "init",
        "doctor",
        "harness",
        "compose",
        "compose-plan",
        "run",
        "lsp",
        "validate",
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

fn harness_command(args: &[String]) -> Result<(), CliError> {
    let manager = HarnessManager::new(CraftHome::from_env()?);
    match args.first().map(String::as_str) {
        Some("install") => {
            let source = args.get(1).ok_or_else(|| {
                CliError::usage("usage: craft harness install github:owner/repo[@ref]")
            })?;
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
                    .map_err(|err| CliError::io("failed to read confirmation", err))?;
            if !confirmed {
                return Err(CliError::Runtime("uninstall cancelled".to_string()));
            }
            let registry = manager.registry()?;
            let harness = registry.uninstall(name, true)?;
            ui::success(format!("uninstalled {}", harness.name));
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
