use std::fs;
use std::path::PathBuf;

use craft_manifest::parse_manifest;

pub fn handle_input(input: &str) -> String {
    let mut output = String::new();
    for message in messages(input) {
        if message.contains("\"method\":\"initialize\"") {
            let id = json_id(&message).unwrap_or_else(|| "null".to_string());
            send(
                &mut output,
                &format!(
                    r#"{{"jsonrpc":"2.0","id":{id},"result":{{"capabilities":{{"textDocumentSync":1,"completionProvider":{{"triggerCharacters":[".","\""]}},"definitionProvider":true}}}}}}"#
                ),
            );
        } else if message.contains("\"method\":\"textDocument/didOpen\"")
            || message.contains("\"method\":\"textDocument/didSave\"")
        {
            let uri = json_string_field(&message, "uri").unwrap_or_default();
            let text = json_string_field(&message, "text").unwrap_or_else(|| {
                file_uri_path(&uri)
                    .and_then(|path| fs::read_to_string(path).ok())
                    .unwrap_or_default()
            });
            publish_diagnostics(&mut output, &uri, &text);
        } else if message.contains("\"method\":\"textDocument/completion\"") {
            let id = json_id(&message).unwrap_or_else(|| "null".to_string());
            send(
                &mut output,
                &format!(
                    r#"{{"jsonrpc":"2.0","id":{id},"result":[{}]}}"#,
                    completion_items().join(",")
                ),
            );
        } else if message.contains("\"method\":\"textDocument/definition\"") {
            let id = json_id(&message).unwrap_or_else(|| "null".to_string());
            send(
                &mut output,
                &format!(r#"{{"jsonrpc":"2.0","id":{id},"result":null}}"#),
            );
        } else if message.contains("\"method\":\"shutdown\"") {
            let id = json_id(&message).unwrap_or_else(|| "null".to_string());
            send(
                &mut output,
                &format!(r#"{{"jsonrpc":"2.0","id":{id},"result":null}}"#),
            );
        }
    }
    output
}

fn messages(input: &str) -> Vec<String> {
    let mut messages = Vec::new();
    let mut offset = 0;
    let bytes = input.as_bytes();

    while let Some(header_end) = find_header_end(&bytes[offset..]) {
        let absolute_header_end = offset + header_end;
        let headers = &input[offset..absolute_header_end];
        let Some(length) = content_length(headers) else {
            break;
        };
        let body_start = absolute_header_end + 4;
        let body_end = body_start + length;
        if bytes.len() < body_end {
            break;
        }
        if let Ok(message) = std::str::from_utf8(&bytes[body_start..body_end]) {
            messages.push(message.to_string());
        }
        offset = body_end;
    }

    messages
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    })
}

fn send(output: &mut String, body: &str) {
    output.push_str(&format!("Content-Length: {}\r\n\r\n{}", body.len(), body));
}

fn publish_diagnostics(output: &mut String, uri: &str, text: &str) {
    let diagnostics = match parse_manifest(text) {
        Ok(_) => Vec::new(),
        Err(error) => vec![format!(
            r#"{{"range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":1}}}},"severity":1,"source":"craft","message":"{}"}}"#,
            json_escape(&error.to_string())
        )],
    };
    send(
        output,
        &format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{{"uri":"{}","diagnostics":[{}]}}}}"#,
            json_escape(uri),
            diagnostics.join(",")
        ),
    );
}

fn completion_items() -> Vec<String> {
    [
        "[harness]",
        "name",
        "version",
        "description",
        "authors",
        "[model]",
        "min_context",
        "recommended",
        "[prompts]",
        "system",
        "[memory]",
        "schema",
        "[tools]",
        "mcp",
        "[validators]",
        "tdd",
    ]
    .iter()
    .map(|label| format!(r#"{{"label":"{}","kind":10}}"#, json_escape(label)))
    .collect()
}

fn json_id(message: &str) -> Option<String> {
    let index = message.find("\"id\"")?;
    let after_key = message[index..].find(':')? + index + 1;
    let rest = message[after_key..].trim_start();
    if rest.starts_with('"') {
        let value = json_string_at(rest)?;
        Some(format!("\"{}\"", json_escape(&value)))
    } else {
        Some(
            rest.chars()
                .take_while(|character| character.is_ascii_digit() || *character == '-')
                .collect(),
        )
        .filter(|value: &String| !value.is_empty())
    }
}

fn json_string_field(message: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let index = message.find(&needle)?;
    let after_key = message[index + needle.len()..].find(':')? + index + needle.len() + 1;
    json_string_at(message[after_key..].trim_start())
}

fn json_string_at(input: &str) -> Option<String> {
    let mut chars = input.chars();
    if chars.next()? != '"' {
        return None;
    }
    let mut output = String::new();
    let mut escaped = false;
    for character in chars {
        if escaped {
            match character {
                'n' => output.push('\n'),
                'r' => output.push('\r'),
                't' => output.push('\t'),
                '"' => output.push('"'),
                '\\' => output.push('\\'),
                other => output.push(other),
            }
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => return Some(output),
            other => output.push(other),
        }
    }
    None
}

fn json_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}

fn file_uri_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(body: &str) -> String {
        format!("Content-Length: {}\r\n\r\n{body}", body.len())
    }

    #[test]
    fn handles_initialize_and_shutdown() {
        let output = handle_input(
            &(packet(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
                + &packet(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#)),
        );

        assert!(output.contains(r#""id":1"#));
        assert!(output.contains(r#""completionProvider""#));
        assert!(output.contains(r#""id":2"#));
    }

    #[test]
    fn publishes_manifest_diagnostics() {
        let output = handle_input(&packet(
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/craft.toml","languageId":"toml","version":1,"text":"[harness]\nname = \"bad\"\n"}}}"#,
        ));

        assert!(output.contains(r#""textDocument/publishDiagnostics""#));
        assert!(output.contains("missing required field"));
    }
}
