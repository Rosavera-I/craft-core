use crate::a2a::{A2AClient, AgentCard, OAuth2MtlsConfig};
use crate::error::{BridgeError, Result};
use crate::mcp::{McpServer, Tool, Transport};
use std::collections::HashMap;

/// Bridge CLI subcommand result
type BridgeResult<T> = Result<T>;

/// Run a bridge command
pub async fn run_bridge_command(args: &[String]) -> BridgeResult<()> {
    match args.first().map(String::as_str) {
        None | Some("--help" | "-h" | "help") => {
            print_bridge_help();
            Ok(())
        }
        Some("serve") => serve_command(&args[1..]).await,
        Some("discover") => discover_command(&args[1..]).await,
        Some("proxy") => proxy_command(&args[1..]).await,
        Some(command) => Err(BridgeError::Protocol(format!(
            "Unknown bridge command: `{}`. Run `craft bridge --help`.",
            command
        ))),
    }
}

fn print_bridge_help() {
    println!(
        "
CRAFT Bridge - A2A and MCP Protocol Bridge

Usage:
  craft bridge serve --protocol <a2a|mcp> [--port PORT] [--stdio]
  craft bridge discover <agent-url>
  craft bridge proxy <harness-name>

Commands:
  serve      Start a protocol server (A2A or MCP)
  discover   Discover an A2A agent at the given URL
  proxy      Expose a CRAFT harness as A2A/MCP agent

Options:
  --protocol a2a|mcp    Protocol to use (required for serve)
  --port PORT          Port for HTTP transport (default: 8080)
  --stdio              Use stdio transport (MCP only)
  --auth-type          Authentication type (oauth2, mtls, apikey)
  --cert PATH          Client certificate for mTLS
  --key PATH           Client key for mTLS
  --ca-cert PATH       CA certificate for mTLS
  --help               Show this help message
"
    );
}

/// Serve command - start a protocol server
async fn serve_command(args: &[String]) -> BridgeResult<()> {
    let protocol = get_flag_value(args, "--protocol")
        .ok_or_else(|| BridgeError::Validation("--protocol is required".to_string()))?;

    match protocol.as_str() {
        "a2a" => serve_a2a(args).await,
        "mcp" => serve_mcp(args).await,
        _ => Err(BridgeError::Validation(format!(
            "Unknown protocol: {}. Use 'a2a' or 'mcp'",
            protocol
        ))),
    }
}

async fn serve_a2a(args: &[String]) -> BridgeResult<()> {
    let port = get_flag_value(args, "--port")
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);

    println!("Starting A2A server on port {}", port);

    let card = AgentCard {
        name: "CRAFT A2A Agent".to_string(),
        description: "A2A-compatible CRAFT agent".to_string(),
        version: "1.0.0".to_string(),
        url: format!("http://127.0.0.1:{}", port),
        capabilities: crate::a2a::types::Capabilities {
            streaming: true,
            push_notifications: false,
            state_transition_history: true,
            history_count: Some(100),
        },
        authentication: None,
        default_input_modes: vec!["text".to_string()],
        default_output_modes: vec!["text".to_string()],
        skills: vec![],
        extras: HashMap::new(),
    };

    let server = crate::a2a::A2AServer::new(card);
    let app = server.router();

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .map_err(BridgeError::Io)?;

    println!("A2A server listening on http://127.0.0.1:{}", port);
    println!(
        "Agent card: http://127.0.0.1:{}/.well-known/agent.json",
        port
    );

    axum::serve(listener, app)
        .await
        .map_err(|e| BridgeError::Http(e.to_string()))?;

    Ok(())
}

async fn serve_mcp(args: &[String]) -> BridgeResult<()> {
    let use_stdio = args.iter().any(|a| a == "--stdio");

    let transport = if use_stdio {
        Transport::Stdio
    } else {
        let port = get_flag_value(args, "--port")
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8080);
        Transport::Http { port }
    };

    println!("Starting MCP server with {:?} transport", transport);

    let server = McpServer::new("CRAFT MCP Server", "1.0.0")
        .with_transport(transport.clone())
        .with_tool(Tool {
            name: "craft.echo".to_string(),
            description: "Echo back the input".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "Message to echo"
                    }
                },
                "required": ["message"]
            }),
            annotations: None,
        })
        .with_resource(crate::mcp::types::Resource {
            uri: "craft://status".to_string(),
            name: "CRAFT Status".to_string(),
            description: Some("CRAFT system status".to_string()),
            mime_type: Some("application/json".to_string()),
            size: None,
            annotations: None,
        });

    server.run().await?;
    Ok(())
}

/// Discover command - discover an A2A agent
async fn discover_command(args: &[String]) -> BridgeResult<()> {
    let url = args
        .first()
        .ok_or_else(|| BridgeError::Validation("Agent URL is required".to_string()))?;

    // Parse auth options
    let auth_type = get_flag_value(args, "--auth-type");

    let client = if let Some(auth) = auth_type {
        match auth.as_str() {
            "oauth2" | "mtls" => {
                let cert_path = get_flag_value(args, "--cert").ok_or_else(|| {
                    BridgeError::Validation("--cert required for mTLS".to_string())
                })?;
                let key_path = get_flag_value(args, "--key").ok_or_else(|| {
                    BridgeError::Validation("--key required for mTLS".to_string())
                })?;
                let ca_cert = get_flag_value(args, "--ca-cert");

                let config = OAuth2MtlsConfig {
                    cert_path,
                    key_path,
                    ca_cert_path: ca_cert,
                };
                A2AClient::discover_with_auth(url, &config).await?
            }
            _ => A2AClient::discover(url).await?,
        }
    } else {
        A2AClient::discover(url).await?
    };

    println!("Discovered A2A Agent:");
    println!("  Name: {}", client.card.name);
    println!("  Description: {}", client.card.description);
    println!("  Version: {}", client.card.version);
    println!("  URL: {}", client.card.url);

    if !client.card.skills.is_empty() {
        println!("  Skills:");
        for skill in &client.card.skills {
            println!("    - {}: {}", skill.name, skill.description);
        }
    }

    if client.card.capabilities.streaming {
        println!("  Supports: Streaming");
    }
    if client.card.capabilities.push_notifications {
        println!("  Supports: Push Notifications");
    }

    Ok(())
}

/// Proxy command - expose a CRAFT harness as A2A/MCP
async fn proxy_command(args: &[String]) -> BridgeResult<()> {
    let harness_name = args
        .first()
        .ok_or_else(|| BridgeError::Validation("Harness name is required".to_string()))?;

    let protocol = get_flag_value(args, "--protocol").unwrap_or_else(|| "a2a".to_string());

    let port = get_flag_value(args, "--port")
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);

    println!(
        "Exposing harness '{}' as {} on port {}",
        harness_name, protocol, port
    );

    // The CLI currently prepares protocol metadata. Harness loading and endpoint binding
    // belong in the next bridge integration pass.

    match protocol.as_str() {
        "a2a" => {
            let card = AgentCard {
                name: format!("CRAFT: {}", harness_name),
                description: format!("A2A proxy for CRAFT harness: {}", harness_name),
                version: "1.0.0".to_string(),
                url: format!("http://127.0.0.1:{}", port),
                capabilities: crate::a2a::types::Capabilities {
                    streaming: true,
                    push_notifications: false,
                    state_transition_history: true,
                    history_count: Some(100),
                },
                authentication: None,
                default_input_modes: vec!["text".to_string()],
                default_output_modes: vec!["text".to_string()],
                skills: vec![crate::a2a::types::Skill {
                    id: harness_name.clone(),
                    name: harness_name.clone(),
                    description: format!("CRAFT harness: {}", harness_name),
                    input_modes: vec!["text".to_string()],
                    output_modes: vec!["text".to_string()],
                }],
                extras: HashMap::new(),
            };

            let server = crate::a2a::A2AServer::new(card);
            let app = server.router();

            let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
                .await
                .map_err(BridgeError::Io)?;

            println!(
                "A2A proxy for '{}' listening on port {}",
                harness_name, port
            );

            axum::serve(listener, app)
                .await
                .map_err(|e| BridgeError::Http(e.to_string()))?;
        }
        "mcp" => {
            let server = McpServer::new(format!("CRAFT: {}", harness_name), "1.0.0")
                .with_transport(Transport::Http { port });

            server.run().await?;
        }
        _ => {
            return Err(BridgeError::Validation(format!(
                "Unknown protocol: {}",
                protocol
            )));
        }
    }

    Ok(())
}

/// Helper function to get flag value from args
fn get_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_flag_value() {
        let args = vec![
            "--protocol".to_string(),
            "a2a".to_string(),
            "--port".to_string(),
            "9000".to_string(),
        ];

        assert_eq!(get_flag_value(&args, "--protocol"), Some("a2a".to_string()));
        assert_eq!(get_flag_value(&args, "--port"), Some("9000".to_string()));
        assert_eq!(get_flag_value(&args, "--missing"), None);
    }
}
