//! # Parallaxis Verifier
//!
//! Checks extracted claims against the Vault.
//! 1. Direct lookup (subject + predicate → match)
//! 2. Mini-reasoner (1-3 hops inference if direct lookup fails)
//! 3. Value comparison with normalization and thresholds

use parallaxis_core::*;
use parallaxis_normalizer::{ValueMatch, values_match};
use parallaxis_vault::Vault;
use tracing::debug;

/// Configuration for the verifier.
pub struct VerifierConfig {
    /// Maximum inference depth (hops in the graph)
    pub max_inference_depth: u8,
    /// Maximum number of claims to verify per request
    pub max_claims: u16,
    /// Timeout in milliseconds
    pub timeout_ms: u64,
}

impl Default for VerifierConfig {
    fn default() -> Self {
        Self {
            max_inference_depth: 3,
            max_claims: 100,
            timeout_ms: 5000,
        }
    }
}

/// The Verifier — checks claims against the Vault.
pub struct Verifier<'v> {
    vault: &'v Vault,
    config: VerifierConfig,
}

impl<'v> Verifier<'v> {
    pub fn new(vault: &'v Vault, config: VerifierConfig) -> Self {
        Self { vault, config }
    }

    /// Verify a single claim against the vault.
    pub fn verify_claim(&self, claim: &Claim) -> VerifiedClaim {
        debug!(
            subject = %claim.subject,
            predicate = %claim.predicate,
            object = %claim.object,
            "Verifying claim"
        );

        // Step 1: Try direct lookup
        if let Some(result) = self.direct_lookup(claim) {
            return result;
        }

        // Step 2: Try mini-reasoner (inference)
        if self.config.max_inference_depth > 0 {
            if let Some(result) = self.infer(claim) {
                return result;
            }
        }

        // Step 3: Not found
        VerifiedClaim {
            original_text: claim.original_text.clone(),
            span_start: claim.span_start,
            span_end: claim.span_end,
            status: VerificationStatus::Unverifiable,
            resolution_method: ResolutionMethod::NotFound,
        }
    }

    /// Verify all claims in an extraction result.
    pub fn verify_all(&self, extraction: &ExtractionResult) -> VerificationResult {
        let claims: Vec<VerifiedClaim> = extraction
            .claims
            .iter()
            .take(self.config.max_claims as usize)
            .map(|c| self.verify_claim(c))
            .collect();

        let (score, coverage) = VerificationResult::compute_metrics(&claims);

        VerificationResult {
            original_text: extraction.original_text.clone(),
            claims,
            score,
            coverage,
            vault_version: self.vault.version.clone(),
            verified_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            disclaimer: "Verification based on Vault data. Not a substitute for professional judgment.".to_string(),
        }
    }

    /// Direct lookup: find entity by subject label, find predicate, check value.
    fn direct_lookup(&self, claim: &Claim) -> Option<VerifiedClaim> {
        let entity = self.vault.find_entity_by_label(&claim.subject)?;
        let predicate = self.vault.find_predicate(&claim.predicate)?;

        let relations = self.vault.lookup(entity.id, predicate.id);
        if relations.is_empty() {
            return None;
        }

        // Check each matching relation
        for relation in &relations {
            // Skip expired temporal facts
            if let Some(until) = relation.valid_until {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                if now > until {
                    continue; // This fact is no longer valid
                }
            }

            let status = self.compare_value(claim, relation, predicate);
            return Some(VerifiedClaim {
                original_text: claim.original_text.clone(),
                span_start: claim.span_start,
                span_end: claim.span_end,
                status,
                resolution_method: ResolutionMethod::DirectLookup,
            });
        }

        None
    }

    /// Compare claim value against vault relation value.
    fn compare_value(
        &self,
        claim: &Claim,
        relation: &Relation,
        predicate: &Predicate,
    ) -> VerificationStatus {
        let tolerance = predicate
            .verification_threshold
            .as_ref()
            .and_then(|t| t.numeric_tolerance)
            .unwrap_or(self.vault.domain.config.numeric_tolerance);

        match &relation.value {
            Value::Text(vault_text) => {
                if vault_text.to_lowercase() == claim.object.to_lowercase() {
                    VerificationStatus::Confirmed {
                        source: relation.source.clone(),
                        confidence: relation.confidence,
                        vault_relation: relation.id,
                    }
                } else {
                    VerificationStatus::Contradicted {
                        vault_value: vault_text.clone(),
                        source: relation.source.clone(),
                        explanation: None,
                    }
                }
            }

            Value::Entity(target_id) => {
                // Check if the claim's object matches the target entity's label
                if let Some(target_entity) = self.vault.get_entity(*target_id) {
                    let matches = target_entity.labels.iter().any(|l| {
                        l.text.to_lowercase() == claim.object.to_lowercase()
                    });
                    if matches {
                        VerificationStatus::Confirmed {
                            source: relation.source.clone(),
                            confidence: relation.confidence,
                            vault_relation: relation.id,
                        }
                    } else {
                        let vault_label = target_entity
                            .labels
                            .iter()
                            .find(|l| l.primary)
                            .map(|l| l.text.clone())
                            .unwrap_or_else(|| format!("EntityId({})", target_id.0));
                        VerificationStatus::Contradicted {
                            vault_value: vault_label,
                            source: relation.source.clone(),
                            explanation: None,
                        }
                    }
                } else {
                    VerificationStatus::Unverifiable
                }
            }

            Value::Number { value: vault_num, unit: _vault_unit } => {
                // Try to parse claim object as number
                let claim_num: f64 = match claim.object.parse() {
                    Ok(n) => n,
                    Err(_) => {
                        // Try extracting number from text like "215 million"
                        return VerificationStatus::Unverifiable;
                    }
                };

                match values_match(claim_num, *vault_num, tolerance) {
                    ValueMatch::Exact => VerificationStatus::Confirmed {
                        source: relation.source.clone(),
                        confidence: relation.confidence,
                        vault_relation: relation.id,
                    },
                    ValueMatch::WithinTolerance { deviation } => VerificationStatus::Imprecise {
                        claim_value: claim.object.clone(),
                        vault_value: format!("{}", vault_num),
                        deviation,
                        source: relation.source.clone(),
                    },
                    ValueMatch::OutOfTolerance { deviation: _ } => {
                        // Check if vault data might be outdated
                        let source_age = relation.source.date.map(|d| {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs() as i64;
                            now - d
                        });

                        if source_age.is_some_and(|age| age > 365 * 24 * 3600) {
                            VerificationStatus::Divergent {
                                claim_value: claim.object.clone(),
                                vault_value: format!("{}", vault_num),
                                vault_source_date: relation
                                    .source
                                    .date
                                    .map(|d| format!("timestamp:{}", d))
                                    .unwrap_or_else(|| "unknown".to_string()),
                                age_warning: "⚠️ Vault data is over 1 year old".to_string(),
                            }
                        } else {
                            VerificationStatus::Contradicted {
                                vault_value: format!("{}", vault_num),
                                source: relation.source.clone(),
                                explanation: None,
                            }
                        }
                    }
                }
            }

            Value::Boolean(vault_bool) => {
                let claim_bool = matches!(
                    claim.object.to_lowercase().as_str(),
                    "true" | "yes" | "sim" | "1"
                );
                if claim_bool == *vault_bool {
                    VerificationStatus::Confirmed {
                        source: relation.source.clone(),
                        confidence: relation.confidence,
                        vault_relation: relation.id,
                    }
                } else {
                    VerificationStatus::Contradicted {
                        vault_value: vault_bool.to_string(),
                        source: relation.source.clone(),
                        explanation: None,
                    }
                }
            }

            // TODO: Coordinate, Date, List comparison
            _ => VerificationStatus::Unverifiable,
        }
    }

    /// Mini-reasoner: try to infer the claim via 1-3 hops.
    ///
    /// Example: Claim "Brasília is in South America"
    /// Hop 1: Brasília → capital_of → Brazil (no match yet)
    /// Hop 2: Brazil → continent → South America (match!)
    /// Returns Confirmed with inference chain [relation1, relation2].
    fn infer(&self, claim: &Claim) -> Option<VerifiedClaim> {
        let entity = self.vault.find_entity_by_label(&claim.subject)?;

        debug!(
            entity = %claim.subject,
            target_predicate = %claim.predicate,
            target_object = %claim.object,
            max_depth = self.config.max_inference_depth,
            "Starting inference"
        );

        // BFS through the graph
        let mut queue: Vec<(EntityId, Vec<RelationId>, u8)> = vec![(entity.id, vec![], 0)];
        let mut visited: std::collections::HashSet<EntityId> = std::collections::HashSet::new();
        visited.insert(entity.id);

        while let Some((current_entity, chain, depth)) = queue.pop() {
            if depth >= self.config.max_inference_depth {
                continue;
            }

            let relations = self.vault.relations_from(current_entity);

            for relation in relations {
                // Check if this relation matches the claim's predicate + object
                let predicate = self.vault.graph.get_predicate(relation.predicate);
                let pred_matches = predicate.map_or(false, |p| {
                    p.name.to_lowercase() == claim.predicate.to_lowercase()
                        || p.aliases.iter().any(|a| a.to_lowercase() == claim.predicate.to_lowercase())
                });

                if pred_matches {
                    // Check if the value matches
                    let value_matches = match &relation.value {
                        Value::Entity(target_id) => {
                            if let Some(target) = self.vault.get_entity(*target_id) {
                                target.labels.iter().any(|l| {
                                    l.text.to_lowercase() == claim.object.to_lowercase()
                                })
                            } else {
                                false
                            }
                        }
                        Value::Text(t) => t.to_lowercase() == claim.object.to_lowercase(),
                        _ => false,
                    };

                    if value_matches {
                        let mut full_chain = chain.clone();
                        full_chain.push(relation.id);

                        debug!(
                            hops = full_chain.len(),
                            "Inference match found"
                        );

                        return Some(VerifiedClaim {
                            original_text: claim.original_text.clone(),
                            span_start: claim.span_start,
                            span_end: claim.span_end,
                            status: VerificationStatus::Confirmed {
                                source: relation.source.clone(),
                                confidence: Confidence::Inferred,
                                vault_relation: relation.id,
                            },
                            resolution_method: ResolutionMethod::Inference {
                                hops: full_chain.len() as u8,
                                chain: full_chain,
                            },
                        });
                    }
                }

                // Follow Entity values to continue the chain
                if let Value::Entity(next_entity) = &relation.value {
                    if !visited.contains(next_entity) {
                        visited.insert(*next_entity);
                        let mut new_chain = chain.clone();
                        new_chain.push(relation.id);
                        queue.push((*next_entity, new_chain, depth + 1));
                    }
                }
            }
        }

        debug!("No inference path found");
        None
    }
}
