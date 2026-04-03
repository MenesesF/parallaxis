//! # Parallaxis Extractor
//!
//! Decomposes LLM output text into structured claims (triples).
//! Supports multiple backends: simple rule-based (testing) and LLM-powered (production).

pub mod llm;

use parallaxis_core::*;

/// Trait for claim extraction backends.
pub trait ExtractorBackend: Send + Sync {
    /// Extract claims from text, using the provided predicate schema.
    fn extract(
        &self,
        text: &str,
        schema: &[Predicate],
    ) -> impl std::future::Future<Output = Result<ExtractionResult>> + Send;
}

/// Simple rule-based extractor for testing (no LLM needed).
pub struct SimpleExtractor;

impl ExtractorBackend for SimpleExtractor {
    async fn extract(
        &self,
        text: &str,
        _schema: &[Predicate],
    ) -> Result<ExtractionResult> {
        Ok(ExtractionResult {
            original_text: text.to_string(),
            claims: vec![Claim {
                original_text: text.to_string(),
                span_start: 0,
                span_end: text.len(),
                subject: String::new(),
                predicate: String::new(),
                object: String::new(),
                conditions: vec![],
                extraction_confidence: 0.0,
            }],
            confidence: 0.0,
            warnings: vec![],
        })
    }
}
