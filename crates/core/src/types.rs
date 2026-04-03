//! Core types: entities, relations, values, domains.

use serde::{Deserialize, Serialize};

// ── Identifiers ──────────────────────────────────────────────

/// Compact entity identifier. u64 internally — never a String.
/// Comparison is 10-100x faster than string comparison.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct EntityId(pub u64);

/// Predicate identifier. u32 is enough for millions of predicate types.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct PredicateId(pub u32);

/// Domain identifier.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct DomainId(pub u32);

/// Relation identifier (position in the vault).
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelationId(pub u64);

/// Rule identifier for inference rules.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuleId(pub u32);

// ── Entity ───────────────────────────────────────────────────

/// A node in the knowledge graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub labels: Vec<Label>,
    pub domain: DomainId,
}

/// Entity type classification.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityKind {
    pub id: u32,
    pub name: String,
    pub parent: Option<u32>,
}

/// A label in a specific language (for multilingual aliases).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Label {
    pub text: String,
    pub language: String, // ISO 639-1: "en", "pt", "ja", ...
    pub primary: bool,    // is this the primary label for this language?
}

// ── Relation (the triple) ────────────────────────────────────

/// A fact in the knowledge graph: subject → predicate → value.
/// Every relation has a source, confidence, temporal bounds, and domain.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Relation {
    pub id: RelationId,
    pub subject: EntityId,
    pub predicate: PredicateId,
    pub value: Value,
    pub confidence: Confidence,
    pub source: SourceRef,
    pub domain: DomainId,
    pub valid_from: Option<i64>,  // unix timestamp, when this became true
    pub valid_until: Option<i64>, // unix timestamp, when this stopped being true (None = still true)
    pub timestamp: i64,           // when this was recorded in the vault
}

// ── Value (typed, not just strings) ──────────────────────────

/// Typed value for relation objects.
/// Enables numeric comparison, unit conversion, and proper matching.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Value {
    /// Free text
    Text(String),

    /// Numeric value with unit (enables normalization and threshold comparison)
    Number {
        value: f64,
        unit: Unit,
    },

    /// Date with variable precision
    Date {
        timestamp: i64,
        precision: DatePrecision,
    },

    /// Boolean fact
    Boolean(bool),

    /// Reference to another entity
    Entity(EntityId),

    /// Geographic coordinates
    Coordinate {
        lat: f64,
        lon: f64,
    },

    /// List of values (e.g., official languages of a country)
    List(Vec<Value>),
}

/// Physical/measurement units (SI-based).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Unit {
    // Dimensionless
    None,
    Percent,

    // Length
    Meter,
    Kilometer,

    // Mass
    Gram,
    Kilogram,

    // Temperature
    Kelvin,
    Celsius,
    Fahrenheit,

    // Time
    Second,
    Year,

    // Area
    SquareMeter,
    SquareKilometer,

    // Population / count
    Count,

    // Currency
    Currency(String), // ISO 4217: "USD", "BRL", "EUR"

    // Speed
    MeterPerSecond,

    // Pressure
    Pascal,

    // Volume
    Liter,
    CubicMeter,

    // Custom unit (extensible)
    Custom(String),
}

/// Date precision levels.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DatePrecision {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

// ── Confidence ───────────────────────────────────────────────

/// How much we trust a fact. Not a probability — a verification level.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// Fundamental truth (mathematics, definitions)
    Axiom,
    /// Verified by multiple independent sources
    Verified,
    /// Attested by one reliable source
    Attested,
    /// Derived by reasoning (inference chain available)
    Inferred,
    /// Exists but not independently verified
    Uncertain,
    /// In quarantine — not yet accepted
    Provisional,
}

// ── Source ────────────────────────────────────────────────────

/// Where a fact came from. Every fact MUST have a source.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceRef {
    pub kind: SourceKind,
    pub name: String,       // "IBGE", "WHO", "PubMed"
    pub locator: String,    // URI, DOI, URL, etc.
    pub date: Option<i64>,  // when the source published this
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SourceKind {
    /// Government or international organization
    Official,
    /// Peer-reviewed publication
    Academic,
    /// Structured knowledge base (Wikidata, PubChem)
    KnowledgeBase,
    /// Community-contributed
    Community,
    /// Inferred by the system
    Derived,
}

// ── Domain ───────────────────────────────────────────────────

/// A knowledge domain. Domains organize the vault and define
/// trust policies, verification thresholds, and cache TTLs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Domain {
    pub id: DomainId,
    pub name: String,
    pub parent: Option<DomainId>,
    pub config: DomainConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DomainConfig {
    /// Cache TTL in seconds for verified claims in this domain
    pub cache_ttl_secs: u64,
    /// Default numeric tolerance for "imprecise" vs "divergent"
    pub numeric_tolerance: f64,
    /// Date tolerance in days
    pub date_tolerance_days: u32,
    /// Coordinate tolerance in kilometers
    pub coordinate_tolerance_km: f64,
}

impl Default for DomainConfig {
    fn default() -> Self {
        Self {
            cache_ttl_secs: 180 * 24 * 3600, // 180 days
            numeric_tolerance: 0.03,           // 3%
            date_tolerance_days: 0,
            coordinate_tolerance_km: 1.0,
        }
    }
}

// ── Predicate ────────────────────────────────────────────────

/// Predicate definition — what kind of relationship this is.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Predicate {
    pub id: PredicateId,
    pub name: String,                    // "capital", "population", "boiling_point"
    pub kind: PredicateKind,
    pub expected_value_type: ValueType,  // what type of Value this predicate expects
    pub expected_unit: Option<Unit>,     // preferred unit for normalization
    pub aliases: Vec<String>,            // "introduced_in" = "released_in" = "available_since"
    pub domain: DomainId,
    pub verification_threshold: Option<VerificationThreshold>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PredicateKind {
    /// Simple attribute (population, area)
    Attribute,
    /// Relationship between entities (capital, border)
    Relation,
    /// Causal link (causes, prevents)
    Causal,
    /// Temporal ordering (precedes, follows)
    Temporal,
    /// Composition (composed_of, contains)
    Compositional,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ValueType {
    Text,
    Number,
    Date,
    Boolean,
    Entity,
    Coordinate,
    List,
    Any,
}

/// Per-predicate verification thresholds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationThreshold {
    /// Numeric tolerance override (e.g., 0.0 for constants, 0.05 for population)
    pub numeric_tolerance: Option<f64>,
    /// Date tolerance override in days
    pub date_tolerance_days: Option<u32>,
    /// Coordinate tolerance override in km
    pub coordinate_tolerance_km: Option<f64>,
}

// ── Conversation State ───────────────────────────────────────

/// Minimal conversation context. Fixed size, never grows unbounded.
/// Used for resolving anaphora ("it", "that country") and maintaining
/// domain context across queries in a session.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConversationState {
    /// Last referenced entities (max 10, FIFO)
    pub entity_stack: Vec<EntityId>,
    /// Active domain for this conversation
    pub active_domain: Option<DomainId>,
    /// Resolved ambiguities in this session ("Mercury" = planet)
    pub resolutions: Vec<(String, EntityId)>,
}

impl ConversationState {
    pub fn push_entity(&mut self, id: EntityId) {
        self.entity_stack.insert(0, id);
        self.entity_stack.truncate(10);
    }
}
