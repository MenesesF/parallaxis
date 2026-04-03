//! In-memory knowledge graph with indexed access.

use std::collections::HashMap;

use parallaxis_core::*;

/// The knowledge graph — entities, relations, predicates, and indexes.
pub struct KnowledgeGraph {
    entities: HashMap<EntityId, Entity>,
    relations: Vec<Relation>,
    predicates: HashMap<PredicateId, Predicate>,

    // Indexes
    /// label (lowercase) → EntityId
    label_index: HashMap<String, EntityId>,
    /// (subject, predicate) → relation indices
    subject_predicate_index: HashMap<(EntityId, PredicateId), Vec<usize>>,
    /// subject → relation indices
    subject_index: HashMap<EntityId, Vec<usize>>,
    /// predicate name/alias (lowercase) → PredicateId
    predicate_name_index: HashMap<String, PredicateId>,
    /// For Entity values: target EntityId → relation indices  
    target_index: HashMap<EntityId, Vec<usize>>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            relations: Vec::new(),
            predicates: HashMap::new(),
            label_index: HashMap::new(),
            subject_predicate_index: HashMap::new(),
            subject_index: HashMap::new(),
            predicate_name_index: HashMap::new(),
            target_index: HashMap::new(),
        }
    }

    pub fn add_entity(&mut self, entity: Entity) {
        // Index all labels (lowercase for case-insensitive lookup)
        for label in &entity.labels {
            let lower = label.text.to_lowercase();
            self.label_index.insert(lower.clone(), entity.id);
            // Also index without accents
            let normalized = remove_accents(&lower);
            if normalized != lower {
                self.label_index.insert(normalized, entity.id);
            }
        }
        self.entities.insert(entity.id, entity);
    }

    pub fn add_relation(&mut self, relation: Relation) {
        let idx = self.relations.len();

        // Index by (subject, predicate)
        self.subject_predicate_index
            .entry((relation.subject, relation.predicate))
            .or_default()
            .push(idx);

        // Index by subject
        self.subject_index
            .entry(relation.subject)
            .or_default()
            .push(idx);

        // Index by target (for Entity values)
        if let Value::Entity(target_id) = &relation.value {
            self.target_index
                .entry(*target_id)
                .or_default()
                .push(idx);
        }

        self.relations.push(relation);
    }

    pub fn add_predicate(&mut self, predicate: Predicate) {
        // Index by name
        self.predicate_name_index
            .insert(predicate.name.to_lowercase(), predicate.id);
        // Index by aliases
        for alias in &predicate.aliases {
            self.predicate_name_index
                .insert(alias.to_lowercase(), predicate.id);
        }
        self.predicates.insert(predicate.id, predicate);
    }

    /// Direct lookup: subject + predicate → matching relations.
    pub fn lookup(&self, subject: EntityId, predicate: PredicateId) -> Vec<&Relation> {
        self.subject_predicate_index
            .get(&(subject, predicate))
            .map(|indices| indices.iter().map(|&i| &self.relations[i]).collect())
            .unwrap_or_default()
    }

    /// Find entity by label text (case-insensitive, accent-insensitive).
    pub fn find_entity_by_label(&self, label: &str) -> Option<&Entity> {
        let lower = label.to_lowercase();
        // Try exact match first
        if let Some(id) = self.label_index.get(&lower) {
            return self.entities.get(id);
        }
        // Try without accents
        let normalized = remove_accents(&lower);
        self.label_index
            .get(&normalized)
            .and_then(|id| self.entities.get(id))
    }

    /// Find predicate by name or alias (case-insensitive).
    pub fn find_predicate(&self, name: &str) -> Option<&Predicate> {
        self.predicate_name_index
            .get(&name.to_lowercase())
            .and_then(|id| self.predicates.get(id))
    }

    pub fn get_entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }

    pub fn get_predicate(&self, id: PredicateId) -> Option<&Predicate> {
        self.predicates.get(&id)
    }

    /// All relations where entity is the subject.
    pub fn relations_from(&self, subject: EntityId) -> Vec<&Relation> {
        self.subject_index
            .get(&subject)
            .map(|indices| indices.iter().map(|&i| &self.relations[i]).collect())
            .unwrap_or_default()
    }

    /// All relations where entity is the object (Entity values only).
    pub fn relations_to(&self, target: EntityId) -> Vec<&Relation> {
        self.target_index
            .get(&target)
            .map(|indices| indices.iter().map(|&i| &self.relations[i]).collect())
            .unwrap_or_default()
    }

    /// Find all entities whose labels appear in the given text.
    /// Returns (EntityId, matched_label) sorted by label length (longest first).
    pub fn find_entities_in_text(&self, text: &str) -> Vec<(EntityId, String)> {
        let mut found: Vec<(EntityId, String)> = Vec::new();
        let normalized = normalize_text(text);

        for (label, &entity_id) in &self.label_index {
            if label.len() >= 3 && normalized.contains(label.as_str()) {
                found.push((entity_id, label.clone()));
            }
        }

        // Sort by label length descending (prefer longer/more specific matches)
        found.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        // Deduplicate by entity ID
        let mut seen = std::collections::HashSet::new();
        found.retain(|(id, _)| seen.insert(*id));
        found
    }

    /// Add a label alias mapping directly: label text → entity ID.
    /// Also adds the label to the entity for persistence.
    pub fn add_label_mapping(&mut self, label: &str, language: &str, entity_id: EntityId) {
        self.label_index.insert(label.to_lowercase(), entity_id);
        // Also add to entity's labels for persistence
        if let Some(entity) = self.entities.get_mut(&entity_id) {
            let already_has = entity.labels.iter().any(|l| l.text.to_lowercase() == label.to_lowercase());
            if !already_has {
                entity.labels.push(Label {
                    text: label.to_string(),
                    language: language.to_string(),
                    primary: true,
                });
            }
        }
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn relation_count(&self) -> usize {
        self.relations.len()
    }

    /// Get all entities (for serialization).
    pub fn all_entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    /// Get all relations (for serialization).
    pub fn all_relations(&self) -> &[Relation] {
        &self.relations
    }

    /// Get all predicates (for serialization).
    pub fn all_predicates(&self) -> impl Iterator<Item = &Predicate> {
        self.predicates.values()
    }
}

/// Normalize text: lowercase + remove accents.
pub fn normalize_text(s: &str) -> String {
    remove_accents(&s.to_lowercase())
}

/// Remove common accents/diacritics for fuzzy matching.
pub fn remove_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            _ => c,
        })
        .collect()
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}
