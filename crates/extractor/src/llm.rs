//! LLM-powered claim extractor.
//!
//! Calls an OpenAI-compatible API to decompose text into (subject, predicate, object) triples.
//! The vault's predicate schema is included in the prompt so extraction aligns with vault structure.
//! Includes self-check: reconstructs claims and compares with original.

use parallaxis_core::*;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::ExtractorBackend;

/// Configuration for the LLM extractor.
#[derive(Clone, Debug)]
pub struct LlmExtractorConfig {
    /// OpenAI-compatible API URL (e.g., "https://api.openai.com/v1/chat/completions")
    pub api_url: String,
    /// API key
    pub api_key: String,
    /// Model name (e.g., "gpt-4o-mini", "claude-3-haiku-20240307")
    pub model: String,
    /// Max tokens for response
    pub max_tokens: u32,
}

/// LLM-powered claim extractor.
pub struct LlmExtractor {
    config: LlmExtractorConfig,
    client: reqwest::Client,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

#[derive(Deserialize, Debug)]
struct ExtractedTriple {
    subject: String,
    predicate: String,
    object: String,
    #[serde(default)]
    conditions: Vec<String>,
    #[serde(default)]
    original_span: String,
}

#[derive(Deserialize, Debug)]
struct ExtractionResponse {
    triples: Vec<ExtractedTriple>,
    #[serde(default)]
    warnings: Vec<String>,
}

impl LlmExtractor {
    pub fn new(config: LlmExtractorConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self { config, client }
    }

    fn build_prompt(&self, text: &str, schema: &[Predicate]) -> String {
        let predicate_list: Vec<String> = schema
            .iter()
            .map(|p| {
                let aliases = if p.aliases.is_empty() {
                    String::new()
                } else {
                    format!(" (aliases: {})", p.aliases.join(", "))
                };
                format!("  - {}{}", p.name, aliases)
            })
            .collect();

        format!(
            r#"You are a factual claim extractor. Your job is to decompose text into atomic factual claims as (subject, predicate, object) triples.

Available predicates in the knowledge base:
{predicates}

Rules:
1. Extract ONLY factual claims (not opinions, questions, or commands)
2. Use predicates from the list above when possible
3. If a claim has conditions/qualifiers, include them in the "conditions" array
4. Include the original text span for each claim in "original_span"
5. If something is an opinion or subjective, skip it

Respond with valid JSON only:
{{
  "triples": [
    {{
      "subject": "entity name",
      "predicate": "predicate_name",
      "object": "value or entity name",
      "conditions": ["condition1", "condition2"],
      "original_span": "the exact text this claim comes from"
    }}
  ],
  "warnings": ["any extraction issues"]
}}

Text to analyze:
"{text}"
"#,
            predicates = predicate_list.join("\n"),
            text = text,
        )
    }

    /// Self-check: reconstruct sentences from triples and compare with original.
    fn self_check(
        &self,
        original: &str,
        triples: &[ExtractedTriple],
    ) -> Vec<ExtractionWarning> {
        let mut warnings = Vec::new();

        for triple in triples {
            let reconstructed = format!(
                "{} {} {}",
                triple.subject, triple.predicate, triple.object
            );

            // Check if the original span is actually in the text
            if !triple.original_span.is_empty()
                && !original
                    .to_lowercase()
                    .contains(&triple.original_span.to_lowercase())
            {
                warnings.push(ExtractionWarning::ContextLoss {
                    original_segment: triple.original_span.clone(),
                    reconstructed,
                });
            }
        }

        // Check for high hedging
        let hedge_words = [
            "possibly", "might", "could", "perhaps", "maybe", "arguably",
            "some suggest", "it is believed", "reportedly", "allegedly",
            "possivelmente", "talvez", "pode ser", "supostamente",
        ];
        let lower = original.to_lowercase();
        let word_count = original.split_whitespace().count() as f64;
        let hedge_count = hedge_words
            .iter()
            .filter(|w| lower.contains(*w))
            .count() as f64;
        let hedge_ratio = if word_count > 0.0 {
            hedge_count / word_count
        } else {
            0.0
        };

        if hedge_ratio > 0.1 {
            warnings.push(ExtractionWarning::HighHedging { hedge_ratio });
        }

        warnings
    }
}

impl ExtractorBackend for LlmExtractor {
    async fn extract(
        &self,
        text: &str,
        schema: &[Predicate],
    ) -> Result<ExtractionResult> {
        let prompt = self.build_prompt(text, schema);

        debug!(model = %self.config.model, text_len = text.len(), "Calling LLM for extraction");

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            max_tokens: self.config.max_tokens,
            temperature: 0.0, // deterministic extraction
        };

        let response = self
            .client
            .post(&self.config.api_url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ParallaxisError::Extraction(format!("LLM API call failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ParallaxisError::Extraction(format!(
                "LLM API returned {}: {}",
                status,
                &body[..body.len().min(500)]
            )));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| ParallaxisError::Extraction(format!("Failed to parse LLM response: {}", e)))?;

        let content = chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        debug!(response_len = content.len(), "LLM response received");

        // Parse the JSON response — try to find JSON in the response
        let json_str = extract_json(&content);
        let extraction: ExtractionResponse = serde_json::from_str(json_str)
            .map_err(|e| {
                warn!(content = %content, "Failed to parse extraction JSON");
                ParallaxisError::Extraction(format!("Failed to parse extraction result: {}", e))
            })?;

        // Convert to our claim format
        let claims: Vec<Claim> = extraction
            .triples
            .iter()
            .map(|t| {
                let span_text = if t.original_span.is_empty() {
                    format!("{} {} {}", t.subject, t.predicate, t.object)
                } else {
                    t.original_span.clone()
                };

                let span_start = text.find(&span_text).unwrap_or(0);
                let span_end = span_start + span_text.len();

                Claim {
                    original_text: span_text,
                    span_start,
                    span_end,
                    subject: t.subject.clone(),
                    predicate: t.predicate.clone(),
                    object: t.object.clone(),
                    conditions: t.conditions.clone(),
                    extraction_confidence: 0.9, // LLM extraction is generally reliable
                }
            })
            .collect();

        // Self-check
        let mut warnings = self.self_check(text, &extraction.triples);
        for w in &extraction.warnings {
            warnings.push(ExtractionWarning::AmbiguousDecomposition {
                text: text.to_string(),
                reason: w.clone(),
            });
        }

        let confidence = if claims.is_empty() { 0.0 } else { 0.85 };

        Ok(ExtractionResult {
            original_text: text.to_string(),
            claims,
            confidence,
            warnings,
        })
    }
}

/// Try to extract JSON from a response that might have markdown code blocks.
fn extract_json(s: &str) -> &str {
    // Try to find ```json ... ``` blocks
    if let Some(start) = s.find("```json") {
        let json_start = start + 7;
        if let Some(end) = s[json_start..].find("```") {
            return s[json_start..json_start + end].trim();
        }
    }
    // Try to find ``` ... ``` blocks
    if let Some(start) = s.find("```") {
        let json_start = start + 3;
        if let Some(end) = s[json_start..].find("```") {
            return s[json_start..json_start + end].trim();
        }
    }
    // Try to find { ... } directly
    if let Some(start) = s.find('{') {
        if let Some(end) = s.rfind('}') {
            return &s[start..=end];
        }
    }
    s.trim()
}
