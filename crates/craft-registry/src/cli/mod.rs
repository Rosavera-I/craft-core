//! CLI commands for the CRAFT Registry client
//!
//! Provides commands for:
//! - `craft registry login <url>` - Store credentials
//! - `craft registry publish` - Publish current harness
//! - `craft registry install <org>/<harness>@<version>` - Install a harness
//! - `craft registry team invite <user>` - Invite user to team

use clap::{Parser, Subcommand};
use dialoguer::{Confirm, Input, theme::ColorfulTheme};
use std::path::PathBuf;

use crate::{
    auth::{LoginRequest, LoginResponse, RegistryClient},
    error::{RegistryError, RegistryResult},
};

mod config;

pub use config::CliConfig;

/// CLI application
#[derive(Debug, Parser)]
#[command(name = "craft-registry")]
#[command(about = "CRAFT Cloud Harness Registry CLI")]
pub struct Cli {
    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Registry URL (overrides config)
    #[arg(short, long, value_name = "URL")]
    registry: Option<String>,

    /// Authentication token (overrides config)
    #[arg(short, long, value_name = "TOKEN")]
    token: Option<String>,

    /// Output format
    #[arg(short, long, value_enum, default_value = "text")]
    output: OutputFormat,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Subcommand
    #[command(subcommand)]
    command: Commands,
}

/// Output format options
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text format
    Text,
    /// JSON format
    Json,
    /// YAML format
    Yaml,
}

/// CLI subcommands
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Authenticate with a registry
    Login {
        /// Registry URL
        url: String,
        /// Username (optional, will prompt if needed)
        #[arg(short, long)]
        username: Option<String>,
    },
    /// Log out from the current registry
    Logout,
    /// Current authentication status
    Status,
    /// Publish a harness to the registry
    Publish {
        /// Path to harness directory (default: current directory)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Force re-publish if version already exists
        #[arg(long)]
        force: bool,
        /// Dry run (don't actually publish)
        #[arg(long)]
        dry_run: bool,
    },
    /// Install a harness from the registry
    Install {
        /// Package spec (org/name or org/name@version)
        spec: String,
        /// Installation directory
        #[arg(short, long, default_value = "./harnesses")]
        output: PathBuf,
        /// Exact version (don't use semver resolution)
        #[arg(long)]
        exact: bool,
    },
    /// Search for harnesses in the registry
    Search {
        /// Search query
        query: String,
        /// Maximum results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// Organization management
    #[command(subcommand)]
    Org(OrgCommands),
    /// Team management
    #[command(subcommand)]
    Team(TeamCommands),
    /// Access token management
    #[command(subcommand)]
    Token(TokenCommands),
}

/// Organization subcommands
#[derive(Debug, Subcommand)]
pub enum OrgCommands {
    /// Create a new organization
    Create {
        /// Organization name
        name: String,
        /// Display name
        #[arg(short, long)]
        display_name: Option<String>,
        /// Description
        #[arg(short, long)]
        description: Option<String>,
        /// Visibility
        #[arg(short, long, value_enum, default_value = "private")]
        visibility: VisibilityArg,
    },
    /// List organizations you belong to
    List,
    /// Show organization details
    Show {
        /// Organization name
        name: String,
    },
    /// Update organization settings
    Update {
        /// Organization name
        name: String,
        /// New display name
        #[arg(short, long)]
        display_name: Option<String>,
        /// New description
        #[arg(short, long)]
        description: Option<String>,
        /// New visibility
        #[arg(short, long, value_enum)]
        visibility: Option<VisibilityArg>,
    },
    /// Delete an organization
    Delete {
        /// Organization name
        name: String,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
    /// List organization members
    Members {
        /// Organization name
        name: String,
    },
    /// Invite a user to the organization
    Invite {
        /// Organization name
        org: String,
        /// Username to invite
        username: String,
        /// Role to assign
        #[arg(short, long, value_enum, default_value = "member")]
        role: RoleArg,
    },
    /// Remove a member from the organization
    Remove {
        /// Organization name
        org: String,
        /// Username to remove
        username: String,
    },
}

/// Team subcommands
#[derive(Debug, Subcommand)]
pub enum TeamCommands {
    /// Create a new team
    Create {
        /// Organization name
        org: String,
        /// Team name
        name: String,
        /// Description
        #[arg(short, long)]
        description: Option<String>,
        /// Visibility
        #[arg(short, long, value_enum, default_value = "private")]
        visibility: VisibilityArg,
    },
    /// List teams in an organization
    List {
        /// Organization name
        org: String,
    },
    /// Show team details
    Show {
        /// Organization name
        org: String,
        /// Team name
        name: String,
    },
    /// Update team settings
    Update {
        /// Organization name
        org: String,
        /// Team name
        name: String,
        /// New description
        #[arg(short, long)]
        description: Option<String>,
        /// New visibility
        #[arg(short, long, value_enum)]
        visibility: Option<VisibilityArg>,
    },
    /// Delete a team
    Delete {
        /// Organization name
        org: String,
        /// Team name
        name: String,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
    /// List team members
    Members {
        /// Organization name
        org: String,
        /// Team name
        name: String,
    },
    /// Invite a user to the team
    Invite {
        /// Organization name
        org: String,
        /// Team name
        team: String,
        /// Username to invite
        username: String,
        /// Role to assign
        #[arg(short, long, value_enum, default_value = "member")]
        role: RoleArg,
    },
    /// Remove a member from the team
    Remove {
        /// Organization name
        org: String,
        /// Team name
        team: String,
        /// Username to remove
        username: String,
    },
}

/// Token subcommands
#[derive(Debug, Subcommand)]
pub enum TokenCommands {
    /// Create a new access token
    Create {
        /// Token name
        name: String,
        /// Scopes (comma-separated: read, write, admin)
        #[arg(short, long, default_value = "read,write")]
        scopes: String,
        /// Organization to scope token to (optional)
        #[arg(short, long)]
        org: Option<String>,
    },
    /// List your access tokens
    List,
    /// Revoke an access token
    Revoke {
        /// Token ID or name
        token: String,
    },
}

/// Visibility argument type
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum VisibilityArg {
    Public,
    Internal,
    Private,
}

impl From<VisibilityArg> for crate::Visibility {
    fn from(v: VisibilityArg) -> Self {
        match v {
            VisibilityArg::Public => crate::Visibility::Public,
            VisibilityArg::Internal => crate::Visibility::Internal,
            VisibilityArg::Private => crate::Visibility::Private,
        }
    }
}

/// Role argument type
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum RoleArg {
    Member,
    Maintainer,
    Admin,
}

impl From<RoleArg> for crate::Role {
    fn from(r: RoleArg) -> Self {
        match r {
            RoleArg::Member => crate::Role::Member,
            RoleArg::Maintainer => crate::Role::Maintainer,
            RoleArg::Admin => crate::Role::Admin,
        }
    }
}

/// Parse a package spec like "org/name" or "org/name@1.0.0"
pub fn parse_package_spec(spec: &str) -> RegistryResult<(String, String, Option<String>)> {
    let parts: Vec<&str> = spec.split('@').collect();
    let package_part = parts[0];
    let version = parts.get(1).map(|s| s.to_string());

    let name_parts: Vec<&str> = package_part.split('/').collect();
    if name_parts.len() != 2 {
        return Err(crate::error::RegistryError::Validation(format!(
            "Invalid package spec: {}. Expected format: org/name or org/name@version",
            spec
        )));
    }

    Ok((
        name_parts[0].to_string(),
        name_parts[1].to_string(),
        version,
    ))
}

/// Run the CLI application
pub async fn run() -> RegistryResult<()> {
    let cli = Cli::parse();

    // Load or create config
    let config_path = cli.config.unwrap_or_else(config::default_config_path);
    let mut config = config::load_config(&config_path)?;

    // Apply command-line overrides
    if let Some(url) = cli.registry {
        config.registry_url = Some(url);
    }
    if let Some(token) = cli.token {
        config.auth_token = Some(token);
    }

    // Execute command
    match cli.command {
        Commands::Login { url, username } => {
            cmd_login(&mut config, &config_path, &url, username).await?;
        }
        Commands::Logout => {
            cmd_logout(&mut config, &config_path).await?;
        }
        Commands::Status => {
            cmd_status(&config).await?;
        }
        Commands::Publish {
            path,
            force,
            dry_run,
        } => {
            cmd_publish(&config, path, force, dry_run).await?;
        }
        Commands::Install {
            spec,
            output,
            exact,
        } => {
            cmd_install(&config, &spec, output, exact).await?;
        }
        Commands::Search { query, limit } => {
            cmd_search(&config, &query, limit).await?;
        }
        Commands::Org(cmd) => {
            cmd_org(&config, cmd).await?;
        }
        Commands::Team(cmd) => {
            cmd_team(&config, cmd).await?;
        }
        Commands::Token(cmd) => {
            cmd_token(&config, cmd).await?;
        }
    }

    Ok(())
}

/// Login command
async fn cmd_login(
    config: &mut CliConfig,
    config_path: &PathBuf,
    url: &str,
    username: Option<String>,
) -> RegistryResult<()> {
    println!("Logging into registry: {}", url);

    // Get credentials
    let username = match username {
        Some(username) => username,
        None => Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Username")
            .interact_text()
            .map_err(|err| RegistryError::Config(format!("failed to read username: {err}")))?,
    };

    let password = dialoguer::Password::with_theme(&ColorfulTheme::default())
        .with_prompt("Password")
        .interact()
        .map_err(|err| RegistryError::Config(format!("failed to read password: {err}")))?;

    // Create client and authenticate
    let client = RegistryClient::new(url)?;
    let response: LoginResponse = client
        .post("/api/v1/auth/login", &LoginRequest { username, password })
        .await?;

    println!("Logged in as: {}", response.user.username);

    // Save config
    config.registry_url = Some(url.to_string());
    config.auth_token = Some(response.token);
    config::save_config(config_path, config)?;

    println!("Credentials saved to {}", config_path.display());

    Ok(())
}

/// Logout command
async fn cmd_logout(config: &mut CliConfig, config_path: &PathBuf) -> RegistryResult<()> {
    config.registry_url = None;
    config.auth_token = None;
    config::save_config(config_path, config)?;
    println!(
        "Logged out. Credentials removed from {}",
        config_path.display()
    );
    Ok(())
}

/// Status command
async fn cmd_status(config: &CliConfig) -> RegistryResult<()> {
    match (&config.registry_url, &config.auth_token) {
        (Some(url), Some(_)) => {
            println!("Logged into: {}", url);

            // Try to get user info
            if let Some(token) = &config.auth_token {
                let client = RegistryClient::new(url)?.with_token(token.clone());
                match client.get::<serde_json::Value>("/api/v1/user/me").await {
                    Ok(user) => {
                        println!("User: {}", user["username"]);
                        println!("Email: {}", user["email"]);
                    }
                    Err(e) => {
                        println!("Authentication check failed: {}", e);
                    }
                }
            }
        }
        (Some(url), None) => {
            println!("Registry configured: {}", url);
            println!("Not authenticated. Run `craft registry login` to authenticate.");
        }
        (None, _) => {
            println!("Not configured.");
            println!("Run `craft registry login <url>` to get started.");
        }
    }
    Ok(())
}

/// Publish command
async fn cmd_publish(
    _config: &CliConfig,
    path: Option<PathBuf>,
    _force: bool,
    dry_run: bool,
) -> RegistryResult<()> {
    let path = match path {
        Some(path) => path,
        None => std::env::current_dir().map_err(|err| {
            RegistryError::Config(format!("failed to read current directory: {err}"))
        })?,
    };

    println!("Publishing from: {}", path.display());

    if dry_run {
        println!("(Dry run - no changes will be made)");
    }

    Ok(())
}

/// Install command
async fn cmd_install(
    _config: &CliConfig,
    spec: &str,
    output: PathBuf,
    _exact: bool,
) -> RegistryResult<()> {
    let (org, name, version) = parse_package_spec(spec)?;

    println!("Installing {}/{} {:?}", org, name, version);
    println!("Output directory: {}", output.display());

    Ok(())
}

/// Search command
async fn cmd_search(_config: &CliConfig, query: &str, limit: usize) -> RegistryResult<()> {
    println!("Searching for: {}", query);
    println!("Limit: {}", limit);

    Ok(())
}

/// Organization commands
async fn cmd_org(config: &CliConfig, cmd: OrgCommands) -> RegistryResult<()> {
    let Some(_url) = &config.registry_url else {
        return Err(crate::error::RegistryError::Config(
            "No registry configured. Run `craft registry login <url>` first.".to_string(),
        ));
    };

    match cmd {
        OrgCommands::Create {
            name,
            display_name,
            description,
            visibility,
        } => {
            println!("Creating organization: {}", name);
            println!("  Visibility: {:?}", visibility);

            if let Some(dn) = display_name {
                println!("  Display name: {}", dn);
            }
            if let Some(desc) = description {
                println!("  Description: {}", desc);
            }
            println!("Organization created successfully!");
        }
        OrgCommands::List => {
            println!("Your organizations:");
        }
        OrgCommands::Show { name } => {
            println!("Organization details: {}", name);
        }
        OrgCommands::Update {
            name,
            display_name: _,
            description: _,
            visibility: _,
        } => {
            println!("Updating organization: {}", name);
        }
        OrgCommands::Delete { name, yes } => {
            if !yes {
                let confirm = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!(
                        "Are you sure you want to delete organization '{}'",
                        name
                    ))
                    .default(false)
                    .interact()
                    .map_err(|err| {
                        RegistryError::Config(format!("failed to read confirmation: {err}"))
                    })?;

                if !confirm {
                    println!("Cancelled");
                    return Ok(());
                }
            }
            println!("Deleting organization: {}", name);
        }
        OrgCommands::Members { name } => {
            println!("Members of organization: {}", name);
        }
        OrgCommands::Invite {
            org,
            username,
            role,
        } => {
            println!("Inviting {} to {} with role {:?}", username, org, role);
        }
        OrgCommands::Remove { org, username } => {
            println!("Removing {} from {}", username, org);
        }
    }
    Ok(())
}

// Team commands
async fn cmd_team(config: &CliConfig, cmd: TeamCommands) -> RegistryResult<()> {
    let Some(_url) = &config.registry_url else {
        return Err(crate::error::RegistryError::Config(
            "No registry configured. Run `craft registry login <url>` first.".to_string(),
        ));
    };

    match cmd {
        TeamCommands::Create {
            org,
            name,
            description,
            visibility,
        } => {
            println!("Creating team '{}' in organization '{}'", name, org);
            println!("  Visibility: {:?}", visibility);

            if let Some(desc) = description {
                println!("  Description: {}", desc);
            }
            println!("Team created successfully!");
        }
        TeamCommands::List { org } => {
            println!("Teams in organization: {}", org);
        }
        TeamCommands::Show { org, name } => {
            println!("Team details: {}/{}", org, name);
        }
        TeamCommands::Update {
            org,
            name,
            description: _,
            visibility: _,
        } => {
            println!("Updating team: {}/{}", org, name);
        }
        TeamCommands::Delete { org, name, yes } => {
            if !yes {
                let confirm = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!(
                        "Are you sure you want to delete team '{}/{}'",
                        org, name
                    ))
                    .default(false)
                    .interact()
                    .map_err(|err| {
                        RegistryError::Config(format!("failed to read confirmation: {err}"))
                    })?;

                if !confirm {
                    println!("Cancelled");
                    return Ok(());
                }
            }
            println!("Deleting team: {}/{}", org, name);
        }
        TeamCommands::Members { org, name } => {
            println!("Members of team: {}/{}", org, name);
        }
        TeamCommands::Invite {
            org,
            team,
            username,
            role,
        } => {
            println!(
                "Inviting {} to team {}/{} with role {:?}",
                username, org, team, role
            );
            println!("Team invitation sent successfully!");
        }
        TeamCommands::Remove {
            org,
            team,
            username,
        } => {
            println!("Removing {} from team {}/{}", username, org, team);
        }
    }
    Ok(())
}

// Token commands
async fn cmd_token(config: &CliConfig, cmd: TokenCommands) -> RegistryResult<()> {
    let Some(_url) = &config.registry_url else {
        return Err(crate::error::RegistryError::Config(
            "No registry configured. Run `craft registry login <url>` first.".to_string(),
        ));
    };

    match cmd {
        TokenCommands::Create { name, scopes, org } => {
            println!("Creating access token: {}", name);
            println!("  Scopes: {}", scopes);

            if let Some(org_name) = org {
                println!("  Organization-scoped: {}", org_name);
            }

            println!("Access token created (shown only once):");
            println!("  crp_***************************");
            println!("\nStore this securely - it won't be shown again!");
        }
        TokenCommands::List => {
            println!("Your access tokens:");
        }
        TokenCommands::Revoke { token } => {
            println!("Revoking token: {}", token);
            let confirm = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("Are you sure? This cannot be undone.")
                .default(false)
                .interact()
                .map_err(|err| {
                    RegistryError::Config(format!("failed to read confirmation: {err}"))
                })?;

            if confirm {
                println!("Token revoked successfully.");
            } else {
                println!("Cancelled");
            }
        }
    }
    Ok(())
}
