use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;

use craft_manifest::parse_manifest;

pub fn run_server(input: impl Read, output: impl Write) -> io::Result<()> {
    LspServer::new(output).run(input)
}

struct LspServer<W> {
    output: W,
    documents: BTreeMap<String, String>,
}

impl<W: Write> LspServer<W> {
    fn new(output: W) -> Self {
        Self {
            output,
            documents: BTreeMap::new(),
        }
    }

    fn run(&mut self, input: impl Read) -> io::Result<()> {
        let mut reader = BufReader::new(input);
        while let Some(message) = read_message(&mut reader)? {
            let should_exit = self.handle_message(&message)?;
            self.output.flush()?;
            if should_exit {
                break;
            }
        }
        Ok(())
    }

    fn handle_message(&mut self, message: &str) -> io::Result<bool> {
        if method_is(message, "initialize") {
            let id = json_id(message).unwrap_or_else(|| "null".to_string());
            self.send(&initialize_response(&id))?;
        } else if method_is(message, "textDocument/didOpen")
            || method_is(message, "textDocument/didChange")
            || method_is(message, "textDocument/didSave")
        {
            let uri = json_string_field(message, "uri").unwrap_or_default();
            let text = json_string_field(message, "text")
                .or_else(|| self.documents.get(&uri).cloned())
                .or_else(|| read_file_uri(&uri))
                .unwrap_or_default();
            if !uri.is_empty() {
                self.documents.insert(uri.clone(), text.clone());
            }
            self.publish_diagnostics(&uri, &text)?;
        } else if method_is(message, "textDocument/completion") {
            let id = json_id(message).unwrap_or_else(|| "null".to_string());
            self.send(&completion_response(&id))?;
        } else if method_is(message, "textDocument/hover") {
            let id = json_id(message).unwrap_or_else(|| "null".to_string());
            self.send(&hover_response(&id, message))?;
        } else if method_is(message, "shutdown") {
            let id = json_id(message).unwrap_or_else(|| "null".to_string());
            self.send(&format!(r#"{{"jsonrpc":"2.0","id":{id},"result":null}}"#))?;
        } else if method_is(message, "exit") {
            return Ok(true);
        }
        Ok(false)
    }

    fn publish_diagnostics(&mut self, uri: &str, text: &str) -> io::Result<()> {
        let diagnostics = match parse_manifest(text) {
            Ok(_) => Vec::new(),
            Err(error) => vec![format!(
                r#"{{"range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":1}}}},"severity":1,"source":"craft","message":"{}"}}"#,
                json_escape(&error.to_string())
            )],
        };
        self.send(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{{"uri":"{}","diagnostics":[{}]}}}}"#,
            json_escape(uri),
            diagnostics.join(",")
        ))
    }

    fn send(&mut self, body: &str) -> io::Result<()> {
        write!(self.output, "Content-Length: {}\r\n\r\n{body}", body.len())
    }
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let Some(length) = content_length else {
        return Ok(None);
    };
    let mut buffer = vec![0_u8; length];
    reader.read_exact(&mut buffer)?;
    Ok(Some(String::from_utf8_lossy(&buffer).into_owned()))
}

fn initialize_response(id: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"result":{{"capabilities":{{"textDocumentSync":{{"openClose":true,"change":1,"save":{{"includeText":true}}}},"completionProvider":{{"triggerCharacters":[".","\""]}},"hoverProvider":true}}}}}}"#
    )
}

fn completion_response(id: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"result":[{}]}}"#,
        completion_items().join(",")
    )
}

fn completion_items() -> Vec<String> {
    FIELD_DOCS
        .iter()
        .map(|field| {
            format!(
                r#"{{"label":"{}","kind":10,"detail":"{}"}}"#,
                json_escape(field.label),
                json_escape(field.detail)
            )
        })
        .collect()
}

fn hover_response(id: &str, message: &str) -> String {
    let label = json_string_field(message, "word").unwrap_or_default();
    let doc = FIELD_DOCS
        .iter()
        .find(|field| field.label == label)
        .or_else(|| {
            FIELD_DOCS
                .iter()
                .find(|field| !field.label.starts_with('['))
        });
    match doc {
        Some(field) => format!(
            r#"{{"jsonrpc":"2.0","id":{id},"result":{{"contents":{{"kind":"markdown","value":"**{}**\n\n{}"}}}}}}"#,
            json_escape(field.label),
            json_escape(field.detail)
        ),
        None => format!(r#"{{"jsonrpc":"2.0","id":{id},"result":null}}"#),
    }
}

fn method_is(message: &str, method: &str) -> bool {
    message.contains(&format!(r#""method":"{method}""#))
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

fn read_file_uri(uri: &str) -> Option<String> {
    uri.strip_prefix("file://")
        .map(PathBuf::from)
        .and_then(|path| fs::read_to_string(path).ok())
}

struct FieldDoc {
    label: &'static str,
    detail: &'static str,
}

const FIELD_DOCS: &[FieldDoc] = &[
    FieldDoc {
        label: "[harness]",
        detail: "Harness identity and package metadata.",
    },
    FieldDoc {
        label: "name",
        detail: "Stable harness slug used for install, compose, and run workflows.",
    },
    FieldDoc {
        label: "version",
        detail: "Semantic version for the harness manifest.",
    },
    FieldDoc {
        label: "description",
        detail: "Short human-readable summary of the harness purpose.",
    },
    FieldDoc {
        label: "authors",
        detail: "List of harness authors.",
    },
    FieldDoc {
        label: "[model]",
        detail: "Model fit requirements and recommendations.",
    },
    FieldDoc {
        label: "min_context",
        detail: "Minimum context window required by the harness.",
    },
    FieldDoc {
        label: "recommended",
        detail: "Ordered list of recommended local model identifiers.",
    },
    FieldDoc {
        label: "[prompts]",
        detail: "Prompt artifact paths relative to the harness root.",
    },
    FieldDoc {
        label: "system",
        detail: "Path to the harness system prompt.",
    },
    FieldDoc {
        label: "[memory]",
        detail: "Memory schema artifact paths.",
    },
    FieldDoc {
        label: "schema",
        detail: "Path to the harness memory schema.",
    },
    FieldDoc {
        label: "[tools]",
        detail: "Tool binding artifact paths.",
    },
    FieldDoc {
        label: "mcp",
        detail: "Path to MCP tool binding configuration.",
    },
    FieldDoc {
        label: "[validators]",
        detail: "Validation artifact paths.",
    },
    FieldDoc {
        label: "tdd",
        detail: "Path to tdd-dsl checks for harness validation.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_initialize_completion_hover_and_diagnostics() {
        let input = packet(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
            + &packet(r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{}}"#)
            + &packet(
                r#"{"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{"word":"min_context"}}"#,
            )
            + &packet(
                r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/craft.toml","text":"[harness]\nname = \"bad\"\n"}}}"#,
            );
        let mut output = Vec::new();

        run_server(input.as_bytes(), &mut output).unwrap_or_else(|err| panic!("{err}"));

        let output = String::from_utf8_lossy(&output);
        assert!(output.contains(r#""hoverProvider":true"#));
        assert!(output.contains(r#""label":"min_context""#));
        assert!(output.contains("Minimum context window"));
        assert!(output.contains("textDocument/publishDiagnostics"));
        assert!(output.contains("missing required field"));
    }

    #[test]
    fn clears_diagnostics_for_valid_manifest() {
        let input = packet(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///tmp/craft.toml","text":"{}"}}}}}}"#,
            json_escape(valid_manifest())
        ));
        let mut output = Vec::new();

        run_server(input.as_bytes(), &mut output).unwrap_or_else(|err| panic!("{err}"));

        let output = String::from_utf8_lossy(&output);
        assert!(output.contains(r#""diagnostics":[]"#));
    }

    fn packet(body: &str) -> String {
        format!("Content-Length: {}\r\n\r\n{body}", body.len())
    }

    fn valid_manifest() -> &'static str {
        r#"[harness]
name = "starter-harness"
version = "0.1.0"
description = "A starter CRAFT expertise harness"
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
    }
}
