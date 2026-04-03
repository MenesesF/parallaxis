//! # Parallaxis Tagger
//!
//! Takes verification results and produces tagged output.
//! Supports "simple" mode (just status) and "explain" mode (detailed).

use parallaxis_core::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OutputMode {
    Simple,
    Explain,
}

/// A tagged segment of the original text.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaggedSegment {
    pub text: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_value: Option<String>,
}

/// Complete tagged output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaggedOutput {
    pub original_text: String,
    pub segments: Vec<TaggedSegment>,
    pub score: f64,
    pub coverage: f64,
    pub vault_version: String,
    pub disclaimer: String,
}

/// Produce tagged output from verification results.
pub fn tag(result: &VerificationResult, mode: &OutputMode) -> TaggedOutput {
    let segments: Vec<TaggedSegment> = result
        .claims
        .iter()
        .map(|claim| to_segment(claim, mode))
        .collect();

    TaggedOutput {
        original_text: result.original_text.clone(),
        segments,
        score: result.score,
        coverage: result.coverage,
        vault_version: result.vault_version.clone(),
        disclaimer: result.disclaimer.clone(),
    }
}

fn to_segment(claim: &VerifiedClaim, mode: &OutputMode) -> TaggedSegment {
    let (status, source, explanation, vault_value) = match &claim.status {
        VerificationStatus::Confirmed {
            source,
            confidence,
            ..
        } => (
            format!("confirmed ({:?})", confidence),
            Some(format!("{} ({})", source.name, source.locator)),
            None,
            None,
        ),

        VerificationStatus::Contradicted {
            vault_value,
            source,
            explanation,
        } => (
            "contradicted".to_string(),
            Some(format!("{} ({})", source.name, source.locator)),
            match mode {
                OutputMode::Explain => explanation.clone(),
                OutputMode::Simple => None,
            },
            Some(vault_value.clone()),
        ),

        VerificationStatus::Imprecise {
            claim_value,
            vault_value,
            deviation,
            source,
        } => (
            format!("imprecise (deviation: {:.1}%)", deviation * 100.0),
            Some(format!("{} ({})", source.name, source.locator)),
            match mode {
                OutputMode::Explain => Some(format!(
                    "Claim: {}, Vault: {}",
                    claim_value, vault_value
                )),
                OutputMode::Simple => None,
            },
            Some(vault_value.clone()),
        ),

        VerificationStatus::Conditional {
            conditions,
            source,
        } => (
            "conditional".to_string(),
            Some(format!("{} ({})", source.name, source.locator)),
            match mode {
                OutputMode::Explain => Some(format!(
                    "Valid if: {}",
                    conditions.join(", ")
                )),
                OutputMode::Simple => None,
            },
            None,
        ),

        VerificationStatus::Outdated {
            was_true_until,
            current_value,
            source,
        } => (
            "outdated".to_string(),
            Some(format!("{} ({})", source.name, source.locator)),
            match mode {
                OutputMode::Explain => Some(format!(
                    "Was true until {}. Current: {}",
                    was_true_until, current_value
                )),
                OutputMode::Simple => None,
            },
            Some(current_value.clone()),
        ),

        VerificationStatus::Oversimplified { nuance, source } => (
            "oversimplified".to_string(),
            Some(format!("{} ({})", source.name, source.locator)),
            match mode {
                OutputMode::Explain => Some(nuance.clone()),
                OutputMode::Simple => None,
            },
            None,
        ),

        VerificationStatus::Divergent {
            claim_value,
            vault_value,
            vault_source_date,
            age_warning,
        } => (
            "divergent".to_string(),
            None,
            match mode {
                OutputMode::Explain => Some(format!(
                    "Claim: {}, Vault: {} (from {}). {}",
                    claim_value, vault_value, vault_source_date, age_warning
                )),
                OutputMode::Simple => Some(age_warning.clone()),
            },
            Some(vault_value.clone()),
        ),

        VerificationStatus::Debunked {
            debunk_explanation,
            sources,
        } => (
            "debunked".to_string(),
            Some(
                sources
                    .iter()
                    .map(|s| format!("{} ({})", s.name, s.locator))
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
            match mode {
                OutputMode::Explain => Some(debunk_explanation.clone()),
                OutputMode::Simple => None,
            },
            None,
        ),

        VerificationStatus::Unverifiable => (
            "unverifiable".to_string(),
            None,
            None,
            None,
        ),

        VerificationStatus::Opinion => (
            "opinion".to_string(),
            None,
            None,
            None,
        ),
    };

    TaggedSegment {
        text: claim.original_text.clone(),
        status,
        source,
        explanation,
        vault_value,
    }
}
