//! A2A Chat Example
//!
//! This example demonstrates how to chat with an external A2A agent
//! using the CRAFT Bridge library.
//!
//! Usage:
//!   cargo run --example a2a_chat -- <agent-url> [message]
//!
//! Example:
//!   cargo run --example a2a_chat -- http://localhost:8080 "Hello, agent!"

use craft_bridge::a2a::{A2AClient, Message, Part, Role};
use std::io::{self, BufRead, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse arguments
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: a2a_chat <agent-url> [message]");
        eprintln!("Example: a2a_chat http://localhost:8080 'Hello, agent!'");
        std::process::exit(1);
    }

    let agent_url = &args[1];
    let initial_message = args.get(2).cloned();

    println!("A2A Chat Client");
    println!("===============");
    println!("Connecting to agent at: {}", agent_url);
    println!();

    // Discover the agent
    let client = match A2AClient::discover(agent_url).await {
        Ok(client) => {
            println!("✅ Connected to: {}", client.card.name);
            println!("   Description: {}", client.card.description);
            println!("   Version: {}", client.card.version);
            if !client.card.skills.is_empty() {
                println!("   Skills:");
                for skill in &client.card.skills {
                    println!("     • {}: {}", skill.name, skill.description);
                }
            }
            println!();
            client
        }
        Err(e) => {
            eprintln!("❌ Failed to discover agent: {}", e);
            std::process::exit(1);
        }
    };

    // If an initial message was provided, send it
    if let Some(msg) = initial_message {
        println!("You: {}", msg);

        let message = Message {
            role: Role::User,
            parts: vec![Part::Text { text: msg }],
        };

        match client.send_task(message).await {
            Ok(task) => {
                println!("Agent: Task created (ID: {})", task.id);
                println!("  Status: {:?}", task.status.state);

                // If task requires input or is still working, poll for status
                let task_id = task.id.clone();

                // Simulate following task status
                match client.get_task(&task_id).await {
                    Ok(updated_task) => {
                        println!("  Current status: {:?}", updated_task.status.state);
                        if let Some(ref artifacts) = updated_task.artifacts {
                            for artifact in artifacts {
                                println!("  Artifact: {}", artifact.name);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("  Error getting task status: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ Failed to send task: {}", e);
            }
        }

        println!();
    }

    // Interactive mode (if stdio is a TTY)
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    // Only enter interactive mode if we have a TTY
    if atty::is(atty::Stream::Stdin) {
        println!("Interactive mode. Type messages below (Ctrl+D to exit):");
        println!();

        let reader = stdin.lock();
        let mut lines = reader.lines();

        loop {
            print!("You: ");
            handle.flush()?;

            let input = lines.next();
            match input {
                Some(Ok(line)) if !line.trim().is_empty() => {
                    let message = Message {
                        role: Role::User,
                        parts: vec![Part::Text { text: line }],
                    };

                    match client.send_task(message).await {
                        Ok(task) => {
                            println!("Agent: Task submitted (ID: {})", task.id);
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                        }
                    }
                }
                Some(Ok(_)) => continue, // Empty input
                Some(Err(e)) => {
                    eprintln!("Error reading input: {}", e);
                    break;
                }
                None => {
                    println!();
                    break; // EOF
                }
            }
        }

        println!("Goodbye!");
    }

    Ok(())
}
