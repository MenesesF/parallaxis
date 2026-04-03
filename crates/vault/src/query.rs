//! Query interface for the vault.

use parallaxis_core::*;

use crate::Vault;

/// A structured query against the vault.
#[derive(Debug)]
pub struct VaultQuery {
    pub subject_label: String,
    pub predicate_name: String,
}

/// Result of a vault query.
#[derive(Debug)]
pub struct VaultQueryResult {
    pub entity: Entity,
    pub predicate: Predicate,
    pub relations: Vec<Relation>,
}

impl Vault {
    /// Execute a structured query: find entity by label, find predicate by name,
    /// return matching relations.
    pub fn query(&self, q: &VaultQuery) -> Result<Option<VaultQueryResult>> {
        // Find entity
        let entity = match self.find_entity_by_label(&q.subject_label) {
            Some(e) => e.clone(),
            None => return Ok(None),
        };

        // Find predicate
        let predicate = match self.find_predicate(&q.predicate_name) {
            Some(p) => p.clone(),
            None => return Ok(None),
        };

        // Lookup relations
        let relations: Vec<Relation> = self
            .lookup(entity.id, predicate.id)
            .into_iter()
            .cloned()
            .collect();

        if relations.is_empty() {
            return Ok(None);
        }

        Ok(Some(VaultQueryResult {
            entity,
            predicate,
            relations,
        }))
    }
}
