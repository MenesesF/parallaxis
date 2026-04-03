//! # Parallaxis Vault
//!
//! Knowledge graph storage engine. Stores entities and relations
//! in an optimized format with indexing for fast lookup.
//!
//! V1: In-memory graph with JSON persistence.
//! V2+: Binary format with mmap for zero-copy access.

pub mod graph;
pub mod index;
pub mod storage;
pub mod query;

use parallaxis_core::*;
use std::path::Path;

pub use graph::KnowledgeGraph;
pub use query::VaultQuery;

/// The Vault — Parallaxis's source of truth.
pub struct Vault {
    pub graph: KnowledgeGraph,
    pub version: String,
    pub domain: Domain,
}

impl Vault {
    /// Create a new empty vault for a domain.
    pub fn new(domain: Domain, version: String) -> Self {
        Self {
            graph: KnowledgeGraph::new(),
            version,
            domain,
        }
    }

    /// Load a vault from a directory.
    pub fn load(path: &Path) -> Result<Self> {
        storage::load_vault(path)
    }

    /// Save the vault to a directory.
    pub fn save(&self, path: &Path) -> Result<()> {
        storage::save_vault(self, path)
    }

    /// Add an entity to the vault.
    pub fn add_entity(&mut self, entity: Entity) {
        self.graph.add_entity(entity);
    }

    /// Add a relation (fact) to the vault.
    pub fn add_relation(&mut self, relation: Relation) {
        self.graph.add_relation(relation);
    }

    /// Add a predicate definition.
    pub fn add_predicate(&mut self, predicate: Predicate) {
        self.graph.add_predicate(predicate);
    }

    /// Look up relations for a subject + predicate combination.
    pub fn lookup(&self, subject: EntityId, predicate: PredicateId) -> Vec<&Relation> {
        self.graph.lookup(subject, predicate)
    }

    /// Find an entity by label (any language).
    pub fn find_entity_by_label(&self, label: &str) -> Option<&Entity> {
        self.graph.find_entity_by_label(label)
    }

    /// Find a predicate by name or alias.
    pub fn find_predicate(&self, name: &str) -> Option<&Predicate> {
        self.graph.find_predicate(name)
    }

    /// Get entity by ID.
    pub fn get_entity(&self, id: EntityId) -> Option<&Entity> {
        self.graph.get_entity(id)
    }

    /// Get all relations where entity is subject (outgoing).
    pub fn relations_from(&self, subject: EntityId) -> Vec<&Relation> {
        self.graph.relations_from(subject)
    }

    /// Get all relations where entity is object (incoming, for Entity values).
    pub fn relations_to(&self, target: EntityId) -> Vec<&Relation> {
        self.graph.relations_to(target)
    }

    /// Total number of entities.
    pub fn entity_count(&self) -> usize {
        self.graph.entity_count()
    }

    /// Total number of relations.
    pub fn relation_count(&self) -> usize {
        self.graph.relation_count()
    }
}
