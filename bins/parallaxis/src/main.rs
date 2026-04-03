//! Parallaxis CLI — verify LLM output against curated knowledge.
//!
//! Usage:
//!   parallaxis verify --vault ./data/geography "Brasília is the capital of Brazil"
//!   parallaxis info --vault ./data/geography

use std::path::PathBuf;

use parallaxis_core::*;
use parallaxis_extractor::{ExtractorBackend, SimpleExtractor};
use parallaxis_tagger::{OutputMode, tag};
use parallaxis_vault::Vault;
use parallaxis_verifier::{Verifier, VerifierConfig};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "verify" => cmd_verify(&args[2..]).await,
        "info" => cmd_info(&args[2..]),
        "demo" => cmd_demo().await,
        "serve" => cmd_serve(&args[2..]).await,
        _ => print_usage(),
    }
}

fn print_usage() {
    eprintln!(
        r#"Parallaxis — Factual verification for LLM output

"Pode não ser tão fluente. Pode não conversar tão bonito.
Mas quando disser algo, você pode confiar — ou pelo menos
auditar por que disse."

USAGE:
  parallaxis verify --vault <path> "<text>"
  parallaxis info --vault <path>
  parallaxis demo
  parallaxis serve --vault <path> [--port 3000]

COMMANDS:
  verify    Verify text against a vault
  info      Show vault statistics
  demo      Run demo with built-in geography data
  serve     Start HTTP API server
"#
    );
}

async fn cmd_verify(args: &[String]) {
    let mut vault_path: Option<PathBuf> = None;
    let mut text = None;
    let mut mode = OutputMode::Simple;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--vault" => {
                vault_path = args.get(i + 1).map(|s| PathBuf::from(s));
                i += 2;
            }
            "--explain" => {
                mode = OutputMode::Explain;
                i += 1;
            }
            _ => {
                text = Some(args[i].to_string());
                i += 1;
            }
        }
    }

    let vault_path = match vault_path {
        Some(p) => p,
        None => {
            eprintln!("Error: --vault <path> is required");
            return;
        }
    };

    let text = match text {
        Some(t) => t,
        None => {
            eprintln!("Error: text to verify is required");
            return;
        }
    };

    // Load vault
    let vault = match Vault::load(&vault_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error loading vault: {}", e);
            return;
        }
    };

    // Extract claims (simple extractor for now)
    let extractor = SimpleExtractor;
    let predicates: Vec<Predicate> = vault.graph.all_predicates().cloned().collect();
    let extraction = match extractor.extract(&text, &predicates).await {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error extracting claims: {}", e);
            return;
        }
    };

    // Verify
    let verifier = Verifier::new(&vault, VerifierConfig::default());
    let result = verifier.verify_all(&extraction);

    // Tag output
    let tagged = tag(&result, &mode);

    // Print
    println!(
        "{}",
        serde_json::to_string_pretty(&tagged).unwrap_or_else(|_| "Error formatting output".into())
    );
}

fn cmd_info(args: &[String]) {
    let mut vault_path: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--vault" => {
                vault_path = args.get(i + 1).map(|s| PathBuf::from(s));
                i += 2;
            }
            _ => i += 1,
        }
    }

    let vault_path = match vault_path {
        Some(p) => p,
        None => {
            eprintln!("Error: --vault <path> is required");
            return;
        }
    };

    let vault = match Vault::load(&vault_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error loading vault: {}", e);
            return;
        }
    };

    println!("Parallaxis Vault Info");
    println!("─────────────────────");
    println!("Version:    {}", vault.version);
    println!("Domain:     {}", vault.domain.name);
    println!("Entities:   {}", vault.entity_count());
    println!("Relations:  {}", vault.relation_count());
}

async fn cmd_serve(args: &[String]) {
    let mut vault_path = None;
    let mut port: u16 = 3000;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--vault" => {
                vault_path = args.get(i + 1).map(|s| PathBuf::from(s));
                i += 2;
            }
            "--port" => {
                if let Some(p) = args.get(i + 1) {
                    port = p.parse().unwrap_or(3000);
                }
                i += 2;
            }
            _ => i += 1,
        }
    }

    let vault_path = match vault_path {
        Some(p) => p,
        None => {
            eprintln!("Error: --vault <path> is required");
            return;
        }
    };

    let vault = match Vault::load(&vault_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error loading vault: {}", e);
            return;
        }
    };

    println!("Parallaxis API Server");
    println!("─────────────────────");
    println!("Vault:    {} ({})", vault.version, vault.domain.name);
    println!("Entities: {}", vault.entity_count());
    println!("Relations: {}", vault.relation_count());
    println!("Port:     {}", port);
    println!();

    if let Err(e) = parallaxis_protocol::serve(vault, port).await {
        eprintln!("Server error: {}", e);
    }
}

/// Demo with built-in geography data.
async fn cmd_demo() {
    println!("Parallaxis Demo — Geography Vault");
    println!("══════════════════════════════════\n");

    let vault = build_demo_vault();

    println!(
        "Vault loaded: {} entities, {} relations\n",
        vault.entity_count(),
        vault.relation_count()
    );

    // Simulate LLM claims
    let test_claims = vec![
        ("Brazil", "capital", "Brasília", "Brasília is the capital of Brazil"),
        ("Brazil", "capital", "Rio de Janeiro", "Rio de Janeiro is the capital of Brazil"),
        ("France", "capital", "Paris", "Paris is the capital of France"),
        ("Japan", "capital", "Beijing", "Beijing is the capital of Japan"),
        ("Brazil", "population", "215000000", "Brazil has a population of 215 million"),
    ];

    let verifier = Verifier::new(&vault, VerifierConfig::default());

    for (subject, predicate, object, text) in test_claims {
        let claim = Claim {
            original_text: text.to_string(),
            span_start: 0,
            span_end: text.len(),
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            conditions: vec![],
            extraction_confidence: 1.0,
        };

        let result = verifier.verify_claim(&claim);

        let status_icon = match &result.status {
            VerificationStatus::Confirmed { .. } => "✓",
            VerificationStatus::Contradicted { .. } => "✗",
            VerificationStatus::Imprecise { .. } => "~",
            VerificationStatus::Divergent { .. } => "⚠",
            _ => "?",
        };

        println!("[{}] \"{}\"", status_icon, text);
        match &result.status {
            VerificationStatus::Confirmed { source, confidence, .. } => {
                println!("    Status: CONFIRMED ({:?})", confidence);
                println!("    Source: {} ({})", source.name, source.locator);
            }
            VerificationStatus::Contradicted { vault_value, source, .. } => {
                println!("    Status: CONTRADICTED");
                println!("    Vault says: {}", vault_value);
                println!("    Source: {} ({})", source.name, source.locator);
            }
            VerificationStatus::Imprecise { claim_value, vault_value, deviation, .. } => {
                println!("    Status: IMPRECISE (deviation: {:.1}%)", deviation * 100.0);
                println!("    Claim: {}, Vault: {}", claim_value, vault_value);
            }
            VerificationStatus::Divergent { claim_value, vault_value, age_warning, .. } => {
                println!("    Status: DIVERGENT");
                println!("    Claim: {}, Vault: {}", claim_value, vault_value);
                println!("    {}", age_warning);
            }
            VerificationStatus::Unverifiable => {
                println!("    Status: UNVERIFIABLE (not in vault)");
            }
            _ => {
                println!("    Status: {:?}", result.status);
            }
        }
        println!();
    }
}

fn build_demo_vault() -> Vault {
    let domain = Domain {
        id: DomainId(1),
        name: "geography".to_string(),
        parent: None,
        config: DomainConfig {
            cache_ttl_secs: 180 * 24 * 3600,
            numeric_tolerance: 0.03,
            date_tolerance_days: 0,
            coordinate_tolerance_km: 1.0,
        },
    };

    let mut vault = Vault::new(domain, "demo-v0.1".to_string());

    // Predicates
    let capital_pred = Predicate {
        id: PredicateId(1),
        name: "capital".to_string(),
        kind: PredicateKind::Relation,
        expected_value_type: ValueType::Entity,
        expected_unit: None,
        aliases: vec!["capital_city".to_string(), "seat_of_government".to_string()],
        domain: DomainId(1),
        verification_threshold: None,
    };

    let population_pred = Predicate {
        id: PredicateId(2),
        name: "population".to_string(),
        kind: PredicateKind::Attribute,
        expected_value_type: ValueType::Number,
        expected_unit: Some(Unit::Count),
        aliases: vec!["inhabitants".to_string(), "pop".to_string()],
        domain: DomainId(1),
        verification_threshold: Some(VerificationThreshold {
            numeric_tolerance: Some(0.05), // 5% for population (changes fast)
            date_tolerance_days: None,
            coordinate_tolerance_km: None,
        }),
    };

    vault.add_predicate(capital_pred);
    vault.add_predicate(population_pred);

    // Entities
    let brazil = Entity {
        id: EntityId(1),
        kind: EntityKind { id: 1, name: "country".to_string(), parent: None },
        labels: vec![
            Label { text: "Brazil".to_string(), language: "en".to_string(), primary: true },
            Label { text: "Brasil".to_string(), language: "pt".to_string(), primary: true },
        ],
        domain: DomainId(1),
    };

    let brasilia = Entity {
        id: EntityId(2),
        kind: EntityKind { id: 2, name: "city".to_string(), parent: None },
        labels: vec![
            Label { text: "Brasília".to_string(), language: "pt".to_string(), primary: true },
            Label { text: "Brasilia".to_string(), language: "en".to_string(), primary: true },
        ],
        domain: DomainId(1),
    };

    let france = Entity {
        id: EntityId(3),
        kind: EntityKind { id: 1, name: "country".to_string(), parent: None },
        labels: vec![
            Label { text: "France".to_string(), language: "en".to_string(), primary: true },
            Label { text: "França".to_string(), language: "pt".to_string(), primary: true },
        ],
        domain: DomainId(1),
    };

    let paris = Entity {
        id: EntityId(4),
        kind: EntityKind { id: 2, name: "city".to_string(), parent: None },
        labels: vec![
            Label { text: "Paris".to_string(), language: "en".to_string(), primary: true },
        ],
        domain: DomainId(1),
    };

    let japan = Entity {
        id: EntityId(5),
        kind: EntityKind { id: 1, name: "country".to_string(), parent: None },
        labels: vec![
            Label { text: "Japan".to_string(), language: "en".to_string(), primary: true },
            Label { text: "Japão".to_string(), language: "pt".to_string(), primary: true },
        ],
        domain: DomainId(1),
    };

    let tokyo = Entity {
        id: EntityId(6),
        kind: EntityKind { id: 2, name: "city".to_string(), parent: None },
        labels: vec![
            Label { text: "Tokyo".to_string(), language: "en".to_string(), primary: true },
            Label { text: "Tóquio".to_string(), language: "pt".to_string(), primary: true },
        ],
        domain: DomainId(1),
    };

    vault.add_entity(brazil);
    vault.add_entity(brasilia);
    vault.add_entity(france);
    vault.add_entity(paris);
    vault.add_entity(japan);
    vault.add_entity(tokyo);

    // Relations (facts)
    let ibge = SourceRef {
        kind: SourceKind::Official,
        name: "IBGE".to_string(),
        locator: "https://ibge.gov.br".to_string(),
        date: Some(1711929600), // 2024-04-01
    };

    let wikidata = SourceRef {
        kind: SourceKind::KnowledgeBase,
        name: "Wikidata".to_string(),
        locator: "https://wikidata.org".to_string(),
        date: Some(1711929600),
    };

    // Brazil → capital → Brasília
    vault.add_relation(Relation {
        id: RelationId(1),
        subject: EntityId(1),
        predicate: PredicateId(1),
        value: Value::Entity(EntityId(2)),
        confidence: Confidence::Verified,
        source: ibge.clone(),
        domain: DomainId(1),
        valid_from: Some(-315619200), // 1960-04-21
        valid_until: None,
        timestamp: 1711929600,
    });

    // Brazil → population → 203.1M (IBGE 2022 census)
    vault.add_relation(Relation {
        id: RelationId(2),
        subject: EntityId(1),
        predicate: PredicateId(2),
        value: Value::Number {
            value: 203_100_000.0,
            unit: Unit::Count,
        },
        confidence: Confidence::Verified,
        source: SourceRef {
            kind: SourceKind::Official,
            name: "IBGE Census 2022".to_string(),
            locator: "https://censo2022.ibge.gov.br".to_string(),
            date: Some(1659312000), // 2022-08-01
        },
        domain: DomainId(1),
        valid_from: Some(1659312000),
        valid_until: None,
        timestamp: 1659312000,
    });

    // France → capital → Paris
    vault.add_relation(Relation {
        id: RelationId(3),
        subject: EntityId(3),
        predicate: PredicateId(1),
        value: Value::Entity(EntityId(4)),
        confidence: Confidence::Verified,
        source: wikidata.clone(),
        domain: DomainId(1),
        valid_from: None,
        valid_until: None,
        timestamp: 1711929600,
    });

    // Japan → capital → Tokyo
    vault.add_relation(Relation {
        id: RelationId(4),
        subject: EntityId(5),
        predicate: PredicateId(1),
        value: Value::Entity(EntityId(6)),
        confidence: Confidence::Verified,
        source: wikidata.clone(),
        domain: DomainId(1),
        valid_from: None,
        valid_until: None,
        timestamp: 1711929600,
    });

    vault
}
