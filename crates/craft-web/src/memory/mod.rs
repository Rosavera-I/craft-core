//! Memory service for the web dashboard
//!
//! This module provides higher-level memory operations specifically
//! for the web interface, including context assembly and search.

use crate::error::WebResult;
use craft_memory::{Memory, MemoryFact, MemoryScope};

/// High-level memory service for web operations
pub struct MemoryService {
    memory: Memory,
}

impl MemoryService {
    /// Create a new memory service instance
    pub fn new() -> WebResult<Self> {
        let memory = Memory::from_env()?;
        Ok(Self { memory })
    }

    /// Search for facts across multiple scopes
    pub fn search(
        &self,
        query: &str,
        scopes: &[MemoryScope],
        limit: usize,
    ) -> WebResult<Vec<MemoryFact>> {
        let facts = self.memory.search(query, scopes)?;
        Ok(facts.into_iter().take(limit).collect())
    }

    /// Get facts organized by scope
    pub fn facts_by_scope(&self) -> WebResult<Vec<ScopeFacts>> {
        let scopes = vec![
            MemoryScope::Global,
            MemoryScope::User,
            MemoryScope::Project,
            MemoryScope::Session,
        ];

        let mut result = Vec::new();
        for scope in scopes {
            let facts = self.memory.inspect(&scope)?;
            if !facts.is_empty() {
                result.push(ScopeFacts {
                    scope: scope.storage_key(),
                    facts,
                });
            }
        }

        Ok(result)
    }

    /// Search with FTS across all default scopes
    pub fn fts_search(&self, query: &str) -> WebResult<Vec<MemoryFact>> {
        self.search(
            query,
            &[
                MemoryScope::Global,
                MemoryScope::User,
                MemoryScope::Project,
                MemoryScope::Session,
            ],
            50,
        )
    }

    /// Get facts for a specific scope
    pub fn scope_facts(&self, scope: &MemoryScope) -> WebResult<Vec<MemoryFact>> {
        Ok(self.memory.inspect(scope)?)
    }
}

/// Facts grouped by scope
pub struct ScopeFacts {
    pub scope: String,
    pub facts: Vec<MemoryFact>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_facts_structure() {
        let facts = ScopeFacts {
            scope: "test".to_string(),
            facts: vec![],
        };
        assert_eq!(facts.scope, "test");
        assert!(facts.facts.is_empty());
    }
}
