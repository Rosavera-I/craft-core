use std::env;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryScope {
    Global,
    User,
    Project,
    Session,
    Harness(String),
}

impl MemoryScope {
    pub fn parse(value: &str) -> Result<Self, MemoryError> {
        match value {
            "global" => Ok(Self::Global),
            "user" => Ok(Self::User),
            "project" => Ok(Self::Project),
            "session" => Ok(Self::Session),
            value => {
                if let Some(name) = value.strip_prefix("harness:") {
                    validate_scope_name(name)?;
                    Ok(Self::Harness(name.to_string()))
                } else if value.is_empty() {
                    Err(MemoryError::InvalidScope(
                        "memory scope must not be empty".to_string(),
                    ))
                } else {
                    validate_scope_name(value)?;
                    Ok(Self::Harness(value.to_string()))
                }
            }
        }
    }

    pub fn storage_key(&self) -> String {
        match self {
            Self::Global => "global".to_string(),
            Self::User => "user".to_string(),
            Self::Project => "project".to_string(),
            Self::Session => "session".to_string(),
            Self::Harness(name) => format!("harness:{name}"),
        }
    }

    fn context_rank(&self) -> u8 {
        match self {
            Self::Global => 0,
            Self::User => 1,
            Self::Project => 2,
            Self::Session => 3,
            Self::Harness(_) => 4,
        }
    }
}

impl fmt::Display for MemoryScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.storage_key())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryFact {
    pub scope: MemoryScope,
    pub key: String,
    pub value: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextItem {
    pub scope: MemoryScope,
    pub key: String,
    pub value: String,
    pub score: i64,
}

#[derive(Debug, Clone)]
pub struct Memory {
    home: PathBuf,
    db_path: PathBuf,
}

impl Memory {
    pub fn open(home: impl Into<PathBuf>) -> Result<Self, MemoryError> {
        let home = home.into();
        fs::create_dir_all(&home)?;
        fs::create_dir_all(home.join("logs"))?;
        let memory = Self {
            db_path: home.join("memory.sqlite3"),
            home,
        };
        memory.ensure_schema()?;
        Ok(memory)
    }

    pub fn from_env() -> Result<Self, MemoryError> {
        let home = if let Some(path) = env::var_os("CRAFT_HOME") {
            PathBuf::from(path)
        } else {
            env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or_else(|| MemoryError::Config("HOME is not set; set CRAFT_HOME".to_string()))?
                .join(".craft")
        };
        Self::open(home)
    }

    pub fn record(
        &self,
        scope: MemoryScope,
        key: impl AsRef<str>,
        value: impl AsRef<str>,
    ) -> Result<MemoryFact, MemoryError> {
        let key = key.as_ref();
        let value = value.as_ref();
        validate_key(key)?;
        let now = unix_timestamp()?;
        let scope_key = scope.storage_key();
        let sql = format!(
            "INSERT OR IGNORE INTO scopes (name, kind, created_at) VALUES ({scope}, {kind}, {now});
             INSERT INTO facts (scope, key, value, created_at) VALUES ({scope}, {key}, {value}, {now});
             INSERT INTO facts_fts (rowid, scope, key, value) VALUES (last_insert_rowid(), {scope}, {key}, {value});
             INSERT INTO events (scope, event_type, payload, created_at) VALUES ({scope}, 'fact.recorded', {payload}, {now});
             INSERT INTO audit_log (action, scope, key, created_at) VALUES ('memory.record', {scope}, {key}, {now});",
            scope = sql_string(&scope_key),
            kind = sql_string(scope_kind(&scope)),
            key = sql_string(key),
            value = sql_string(value),
            payload = sql_string(&format!(
                "{{\"key\":{},\"value\":{}}}",
                json_string(key),
                json_string(value)
            )),
        );
        self.exec(&sql)?;
        self.append_event(&scope, "fact.recorded", key, value)?;
        self.rotate_logs()?;
        Ok(MemoryFact {
            scope,
            key: key.to_string(),
            value: value.to_string(),
            created_at: now,
        })
    }

    pub fn recall(
        &self,
        scope: &MemoryScope,
        query: impl AsRef<str>,
    ) -> Result<Vec<MemoryFact>, MemoryError> {
        let query = query.as_ref();
        let like = format!("%{}%", escape_like(query));
        let sql = format!(
            "SELECT scope, key, value, created_at
             FROM facts
             WHERE scope = {} AND (key LIKE {} ESCAPE '\\' OR value LIKE {} ESCAPE '\\')
             ORDER BY created_at DESC, id DESC;",
            sql_string(&scope.storage_key()),
            sql_string(&like),
            sql_string(&like),
        );
        self.query_facts(&sql)
    }

    pub fn search(
        &self,
        query: impl AsRef<str>,
        scopes: &[MemoryScope],
    ) -> Result<Vec<MemoryFact>, MemoryError> {
        let query = query.as_ref();
        let fts_query = fts_query(query);
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }
        let scope_filter = if scopes.is_empty() {
            String::new()
        } else {
            format!(
                "AND f.scope IN ({})",
                scopes
                    .iter()
                    .map(|scope| sql_string(&scope.storage_key()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let sql = format!(
            "SELECT f.scope, f.key, f.value, f.created_at
             FROM facts_fts
             JOIN facts f ON f.id = facts_fts.rowid
             WHERE facts_fts MATCH {} {scope_filter}
             ORDER BY bm25(facts_fts), f.created_at DESC
             LIMIT 50;",
            sql_string(&fts_query),
        );
        self.query_facts(&sql)
    }

    pub fn inspect(&self, scope: &MemoryScope) -> Result<Vec<MemoryFact>, MemoryError> {
        let sql = format!(
            "SELECT scope, key, value, created_at
             FROM facts
             WHERE scope = {}
             ORDER BY created_at DESC, id DESC;",
            sql_string(&scope.storage_key()),
        );
        self.query_facts(&sql)
    }

    pub fn assemble_context(
        &self,
        scopes: &[MemoryScope],
        query: impl AsRef<str>,
        max_tokens: usize,
    ) -> Result<Vec<ContextItem>, MemoryError> {
        let query = query.as_ref().to_ascii_lowercase();
        let mut items = Vec::new();
        for scope in scopes {
            for fact in self.inspect(scope)? {
                let relevance = relevance_score(&fact, &query);
                let score = i64::from(scope.context_rank()) * 10_000 + relevance + fact.created_at;
                items.push(ContextItem {
                    scope: fact.scope,
                    key: fact.key,
                    value: fact.value,
                    score,
                });
            }
        }

        items.sort_by(|left, right| right.score.cmp(&left.score));
        let mut used_tokens = 0;
        let mut selected = Vec::new();
        for item in items {
            let estimate = estimate_tokens(&item.key) + estimate_tokens(&item.value);
            if used_tokens + estimate > max_tokens && !selected.is_empty() {
                continue;
            }
            used_tokens += estimate;
            selected.push(item);
            if used_tokens >= max_tokens {
                break;
            }
        }
        selected.sort_by(|left, right| {
            left.scope
                .context_rank()
                .cmp(&right.scope.context_rank())
                .then_with(|| left.key.cmp(&right.key))
        });
        Ok(selected)
    }

    fn ensure_schema(&self) -> Result<(), MemoryError> {
        self.exec(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS scopes (
                name TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                project_path TEXT,
                started_at INTEGER NOT NULL,
                ended_at INTEGER
             );
             CREATE TABLE IF NOT EXISTS facts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scope TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY(scope) REFERENCES scopes(name)
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS facts_fts USING fts5(scope, key, value);
             CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scope TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS harness_state (
                harness TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY(harness, key)
             );
             CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                action TEXT NOT NULL,
                scope TEXT NOT NULL,
                key TEXT,
                created_at INTEGER NOT NULL
             );",
        )
    }

    fn append_event(
        &self,
        scope: &MemoryScope,
        event_type: &str,
        key: &str,
        value: &str,
    ) -> Result<(), MemoryError> {
        let date = current_date();
        let path = self.home.join("logs").join(format!("events-{date}.jsonl"));
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(
            file,
            "{{\"timestamp\":{},\"scope\":{},\"event_type\":{},\"payload\":{{\"key\":{},\"value\":{}}}}}",
            unix_timestamp()?,
            json_string(&scope.storage_key()),
            json_string(event_type),
            json_string(key),
            json_string(value)
        )?;
        Ok(())
    }

    fn rotate_logs(&self) -> Result<(), MemoryError> {
        let gzip_available = Command::new("gzip")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if !gzip_available {
            return Ok(());
        }

        let cutoff = unix_timestamp()? - 7 * 24 * 60 * 60;
        for entry in fs::read_dir(self.home.join("logs"))? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
                continue;
            }
            let modified = entry
                .metadata()?
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(cutoff + 1);
            if modified < cutoff {
                let status = Command::new("gzip").arg("-f").arg(&path).status()?;
                if !status.success() {
                    return Err(MemoryError::CommandFailed(format!(
                        "gzip failed for {}",
                        path.display()
                    )));
                }
            }
        }
        Ok(())
    }

    fn exec(&self, sql: &str) -> Result<(), MemoryError> {
        let output = Command::new("sqlite3")
            .arg("-batch")
            .arg(&self.db_path)
            .arg(sql)
            .output()
            .map_err(|err| MemoryError::CommandFailed(format!("failed to run sqlite3: {err}")))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(MemoryError::Storage(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ))
        }
    }

    fn query_facts(&self, sql: &str) -> Result<Vec<MemoryFact>, MemoryError> {
        let output = Command::new("sqlite3")
            .arg("-batch")
            .arg("-separator")
            .arg("\t")
            .arg(&self.db_path)
            .arg(sql)
            .output()
            .map_err(|err| MemoryError::CommandFailed(format!("failed to run sqlite3: {err}")))?;
        if !output.status.success() {
            return Err(MemoryError::Storage(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        parse_fact_rows(&String::from_utf8_lossy(&output.stdout))
    }
}

#[derive(Debug)]
pub enum MemoryError {
    Config(String),
    InvalidScope(String),
    InvalidKey(String),
    Io(String),
    CommandFailed(String),
    Storage(String),
    Time(String),
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemoryError::Config(message)
            | MemoryError::InvalidScope(message)
            | MemoryError::InvalidKey(message)
            | MemoryError::Io(message)
            | MemoryError::CommandFailed(message)
            | MemoryError::Storage(message)
            | MemoryError::Time(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for MemoryError {}

impl From<std::io::Error> for MemoryError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

fn parse_fact_rows(output: &str) -> Result<Vec<MemoryFact>, MemoryError> {
    let mut facts = Vec::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let parts = line.split('\t').collect::<Vec<_>>();
        if parts.len() != 4 {
            return Err(MemoryError::Storage(format!(
                "unexpected memory row shape: {line}"
            )));
        }
        facts.push(MemoryFact {
            scope: MemoryScope::parse(parts[0])?,
            key: parts[1].to_string(),
            value: parts[2].to_string(),
            created_at: parts[3]
                .parse()
                .map_err(|_| MemoryError::Storage(format!("invalid timestamp in row: {line}")))?,
        });
    }
    Ok(facts)
}

fn unix_timestamp() -> Result<i64, MemoryError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| MemoryError::Time(err.to_string()))?
        .as_secs() as i64)
}

fn current_date() -> String {
    Command::new("date")
        .arg("+%F")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown-date".to_string())
}

fn validate_key(key: &str) -> Result<(), MemoryError> {
    if key.trim().is_empty() {
        Err(MemoryError::InvalidKey(
            "memory key must not be empty".to_string(),
        ))
    } else {
        Ok(())
    }
}

fn validate_scope_name(name: &str) -> Result<(), MemoryError> {
    if name.is_empty()
        || !name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        Err(MemoryError::InvalidScope(format!(
            "invalid memory scope `{name}`"
        )))
    } else {
        Ok(())
    }
}

fn scope_kind(scope: &MemoryScope) -> &str {
    match scope {
        MemoryScope::Global => "global",
        MemoryScope::User => "user",
        MemoryScope::Project => "project",
        MemoryScope::Session => "session",
        MemoryScope::Harness(_) => "harness",
    }
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn fts_query(value: &str) -> String {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn relevance_score(fact: &MemoryFact, query: &str) -> i64 {
    if query.is_empty() {
        return 0;
    }
    let key = fact.key.to_ascii_lowercase();
    let value = fact.value.to_ascii_lowercase();
    let mut score = 0;
    if key.contains(query) {
        score += 5_000;
    }
    if value.contains(query) {
        score += 2_500;
    }
    score
}

fn estimate_tokens(value: &str) -> usize {
    (value.len() / 4).max(1)
}

#[derive(Debug, Default)]
pub struct ScopedMemory {
    facts: Vec<MemoryFact>,
}

impl ScopedMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, scope: MemoryScope, key: impl Into<String>, value: impl Into<String>) {
        self.facts.push(MemoryFact {
            scope,
            key: key.into(),
            value: value.into(),
            created_at: 0,
        });
    }

    pub fn recall(&self, scope: &MemoryScope, query: &str) -> Vec<&MemoryFact> {
        self.facts
            .iter()
            .filter(|fact| &fact.scope == scope)
            .filter(|fact| fact.key.contains(query) || fact.value.contains(query))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn recalls_facts_by_scope() {
        let mut memory = ScopedMemory::new();
        memory.record(MemoryScope::Project, "language", "rust");
        memory.record(MemoryScope::Session, "language", "zig");

        let facts = memory.recall(&MemoryScope::Project, "rust");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].value, "rust");
    }

    #[test]
    fn persists_and_searches_memory() {
        let root = temp_root("craft-memory");
        let memory = Memory::open(&root).unwrap_or_else(|err| panic!("{err}"));

        memory
            .record(MemoryScope::Project, "language", "rust")
            .unwrap_or_else(|err| panic!("{err}"));
        memory
            .record(MemoryScope::Session, "language", "zig")
            .unwrap_or_else(|err| panic!("{err}"));

        let project = memory
            .recall(&MemoryScope::Project, "rust")
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(project.len(), 1);
        assert_eq!(project[0].scope, MemoryScope::Project);

        let search = memory
            .search("zig", &[MemoryScope::Session])
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].value, "zig");

        assert!(root.join("memory.sqlite3").is_file());
        assert!(root.join("logs").is_dir());

        fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
    }

    #[test]
    fn assembles_context_in_scope_order_with_budget() {
        let root = temp_root("craft-context");
        let memory = Memory::open(&root).unwrap_or_else(|err| panic!("{err}"));
        memory
            .record(MemoryScope::Global, "style", "concise")
            .unwrap_or_else(|err| panic!("{err}"));
        memory
            .record(MemoryScope::Project, "language", "rust")
            .unwrap_or_else(|err| panic!("{err}"));
        memory
            .record(
                MemoryScope::Harness("godot-designer".to_string()),
                "engine",
                "godot",
            )
            .unwrap_or_else(|err| panic!("{err}"));

        let items = memory
            .assemble_context(
                &[
                    MemoryScope::Global,
                    MemoryScope::Project,
                    MemoryScope::Harness("godot-designer".to_string()),
                ],
                "rust",
                200,
            )
            .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(
            items.first().map(|item| &item.scope),
            Some(&MemoryScope::Global)
        );
        assert!(items.iter().any(|item| item.key == "language"));

        fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
    }

    fn temp_root(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|err| panic!("{err}"))
            .as_nanos();
        env::temp_dir().join(format!("{prefix}-{unique}"))
    }
}
