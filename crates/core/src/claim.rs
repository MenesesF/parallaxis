//! Claims — extracted factual assertions from LLM text.

use serde::{Deserialize, Serialize};

/// A factual claim extracted from LLM output.
/// Structured as a triple: (subject, predicate, object/value).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claim {
    /// The original text span this claim was extracted from
    pub original_text: String,

    /// Start position in the original text (byte offset)
    pub span_start: usize,

    /// End position in the original text (byte offset)
    pub span_end: usize,

    /// Extracted subject (e.g., "Brazil", "water", "ibuprofeno")
    pub subject: String,

    /// Extracted predicate/relation (e.g., "capital", "boiling_point")
    pub predicate: String,

    /// Extracted object/value (e.g., "Brasília", "100°C")
    pub object: String,

    /// Conditions/qualifiers extracted (e.g., "at sea level", "for adults")
    pub conditions: Vec<String>,

    /// Confidence of the extraction itself (not the fact — the parsing quality)
    pub extraction_confidence: f64,
}

/// Result of the extraction process.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtractionResult {
    /// Original text that was analyzed
    pub original_text: String,

    /// Extracted claims
    pub claims: Vec<Claim>,

    /// Overall extraction confidence (0.0 - 1.0)
    pub confidence: f64,

    /// Warnings about potential extraction issues
    pub warnings: Vec<ExtractionWarning>,
}

/// Warnings the extractor can emit about text quality or extraction issues.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ExtractionWarning {
    /// Text has high proportion of hedge words (possibly intentionally vague)
    HighHedging {
        hedge_ratio: f64,
    },

    /// Citation appears fabricated (not found in any indexed source)
    FakeSource {
        citation: String,
    },

    /// Vague appeal to authority without identification
    AppealToAuthority {
        phrase: String,
    },

    /// Context may have been lost during extraction
    ContextLoss {
        original_segment: String,
        reconstructed: String,
    },

    /// Claim could not be decomposed into a clean triple
    AmbiguousDecomposition {
        text: String,
        reason: String,
    },
}
