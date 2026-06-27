use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryScope {
    Global,
    User,
    Project,
    Session,
    Harness(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryFact {
    pub scope: MemoryScope,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Default)]
pub struct ScopedMemory {
    facts: BTreeMap<MemoryScope, Vec<MemoryFact>>,
}

impl ScopedMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, scope: MemoryScope, key: impl Into<String>, value: impl Into<String>) {
        let fact = MemoryFact {
            scope: scope.clone(),
            key: key.into(),
            value: value.into(),
        };
        self.facts.entry(scope).or_default().push(fact);
    }

    pub fn recall(&self, scope: &MemoryScope, query: &str) -> Vec<&MemoryFact> {
        self.facts
            .get(scope)
            .into_iter()
            .flatten()
            .filter(|fact| fact.key.contains(query) || fact.value.contains(query))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recalls_facts_by_scope() {
        let mut memory = ScopedMemory::new();
        memory.record(MemoryScope::Project, "language", "rust");
        memory.record(MemoryScope::Session, "language", "zig");

        let facts = memory.recall(&MemoryScope::Project, "rust");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].value, "rust");
    }
}
