//! Verification results — the core output of Parallaxis.

use serde::{Deserialize, Serialize};

use crate::types::{Confidence, RelationId, SourceRef};

/// Verification status for a single claim.
/// This is NOT binary. Real-world facts have nuance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VerificationStatus {
    /// Vault confirms the claim. Source is traceable.
    Confirmed {
        source: SourceRef,
        confidence: Confidence,
        vault_relation: RelationId,
    },

    /// Vault explicitly contradicts the claim.
    Contradicted {
        vault_value: String,
        source: SourceRef,
        explanation: Option<String>,
    },

    /// Value is close but not exact (within threshold).
    Imprecise {
        claim_value: String,
        vault_value: String,
        deviation: f64, // percentage deviation
        source: SourceRef,
    },

    /// True under specific conditions that the claim omitted.
    Conditional {
        conditions: Vec<String>,
        source: SourceRef,
    },

    /// Was true, but vault has newer data.
    Outdated {
        was_true_until: String,
        current_value: String,
        source: SourceRef,
    },

    /// Correct but missing important context.
    Oversimplified {
        nuance: String,
        source: SourceRef,
    },

    /// Values differ, but vault data may be outdated too.
    /// Never say "contradicted" when vault data is old.
    Divergent {
        claim_value: String,
        vault_value: String,
        vault_source_date: String,
        age_warning: String,
    },

    /// Famously false claim with explicit debunk available.
    Debunked {
        debunk_explanation: String,
        sources: Vec<SourceRef>,
    },

    /// Vault has no data on this. Not "false" — unknown.
    Unverifiable,

    /// Subjective statement, not factually verifiable.
    Opinion,
}

/// A fully verified claim — the primary output of the Verifier.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerifiedClaim {
    /// The original extracted claim
    pub original_text: String,

    /// Position in original text
    pub span_start: usize,
    pub span_end: usize,

    /// Verification result
    pub status: VerificationStatus,

    /// Was this resolved by direct lookup or inference?
    pub resolution_method: ResolutionMethod,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ResolutionMethod {
    /// Direct triple match in the vault
    DirectLookup,
    /// Inferred via chain of relations (1-3 hops)
    Inference {
        hops: u8,
        chain: Vec<RelationId>,
    },
    /// Matched from cache
    Cached,
    /// Could not resolve
    NotFound,
}

/// Complete verification result for a text.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Original text that was verified
    pub original_text: String,

    /// Per-claim verification results
    pub claims: Vec<VerifiedClaim>,

    /// Overall confidence score (0.0 - 1.0)
    pub score: f64,

    /// What percentage of claims the vault could verify
    pub coverage: f64,

    /// Vault version used
    pub vault_version: String,

    /// Timestamp of verification
    pub verified_at: i64,

    /// Disclaimer (always present)
    pub disclaimer: String,
}

impl VerificationResult {
    /// Compute score and coverage from claims.
    pub fn compute_metrics(claims: &[VerifiedClaim]) -> (f64, f64) {
        if claims.is_empty() {
            return (1.0, 0.0);
        }

        let total = claims.len() as f64;
        let mut confirmed = 0.0;
        let mut verifiable = 0.0;

        for claim in claims {
            match &claim.status {
                VerificationStatus::Confirmed { .. } => {
                    confirmed += 1.0;
                    verifiable += 1.0;
                }
                VerificationStatus::Imprecise { .. } => {
                    confirmed += 0.8;
                    verifiable += 1.0;
                }
                VerificationStatus::Conditional { .. } => {
                    confirmed += 0.7;
                    verifiable += 1.0;
                }
                VerificationStatus::Oversimplified { .. } => {
                    confirmed += 0.6;
                    verifiable += 1.0;
                }
                VerificationStatus::Contradicted { .. } => {
                    verifiable += 1.0;
                }
                VerificationStatus::Debunked { .. } => {
                    verifiable += 1.0;
                }
                VerificationStatus::Outdated { .. } => {
                    confirmed += 0.3;
                    verifiable += 1.0;
                }
                VerificationStatus::Divergent { .. } => {
                    confirmed += 0.5;
                    verifiable += 1.0;
                }
                VerificationStatus::Unverifiable => {}
                VerificationStatus::Opinion => {}
            }
        }

        let score = if verifiable > 0.0 {
            confirmed / verifiable
        } else {
            0.0
        };
        let coverage = verifiable / total;

        (score, coverage)
    }
}
