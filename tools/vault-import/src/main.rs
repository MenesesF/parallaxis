//! Vault Import Tool — fetches geography data from Wikidata SPARQL endpoint.

use std::collections::HashMap;
use std::path::PathBuf;

use parallaxis_core::*;
use parallaxis_vault::Vault;
use tracing::{error, info, warn};

const WIKIDATA_SPARQL: &str = "https://query.wikidata.org/sparql";
const USER_AGENT: &str = "Parallaxis/0.1 (https://github.com/MenesesF/parallaxis) vault-import";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let output_path = PathBuf::from("data/geography");

    info!("Parallaxis Vault Import — Geography");
    info!("Output: {}", output_path.display());

    match import_geography(&output_path).await {
        Ok(()) => info!("Import complete!"),
        Err(e) => error!("Import failed: {}", e),
    }
}

async fn import_geography(output_path: &PathBuf) -> Result<()> {
    let domain = Domain {
        id: DomainId(1),
        name: "geography".to_string(),
        parent: None,
        config: DomainConfig {
            cache_ttl_secs: 180 * 24 * 3600,
            numeric_tolerance: 0.05,
            date_tolerance_days: 0,
            coordinate_tolerance_km: 1.0,
        },
    };

    let mut vault = Vault::new(domain, "geography-v2026Q2".to_string());

    // Add predicates
    add_predicates(&mut vault);

    // Fetch data from Wikidata
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| ParallaxisError::Vault(e.to_string()))?;

    info!("Fetching countries...");
    fetch_countries(&client, &mut vault).await?;

    info!("Fetching Portuguese labels...");
    fetch_pt_labels(&client, &mut vault).await?;

    info!("Fetching capitals...");
    fetch_capitals(&client, &mut vault).await?;

    info!("Fetching borders...");
    fetch_borders(&client, &mut vault).await?;

    info!("Fetching Brazilian states...");
    fetch_brazilian_states(&client, &mut vault).await?;

    info!("Fetching continents...");
    fetch_continents(&client, &mut vault).await?;

    info!(
        "Vault built: {} entities, {} relations",
        vault.entity_count(),
        vault.relation_count()
    );

    // Save
    vault.save(output_path)?;
    info!("Saved to {}", output_path.display());

    Ok(())
}

fn add_predicates(vault: &mut Vault) {
    let preds = vec![
        ("capital", PredicateKind::Relation, ValueType::Entity, vec!["capital_city", "seat_of_government"]),
        ("population", PredicateKind::Attribute, ValueType::Number, vec!["inhabitants", "pop"]),
        ("area", PredicateKind::Attribute, ValueType::Number, vec!["size", "surface_area"]),
        ("continent", PredicateKind::Relation, ValueType::Entity, vec!["part_of_continent"]),
        ("official_language", PredicateKind::Relation, ValueType::Entity, vec!["language", "official_lang"]),
        ("currency", PredicateKind::Relation, ValueType::Entity, vec!["money", "official_currency"]),
        ("coordinates", PredicateKind::Attribute, ValueType::Coordinate, vec!["location", "coords", "geo"]),
        ("borders", PredicateKind::Relation, ValueType::Entity, vec!["shares_border_with", "adjacent_to", "neighbor"]),
        ("country", PredicateKind::Relation, ValueType::Entity, vec!["country_of", "nation"]),
    ];

    for (i, (name, kind, vtype, aliases)) in preds.into_iter().enumerate() {
        vault.add_predicate(Predicate {
            id: PredicateId((i + 1) as u32),
            name: name.to_string(),
            kind,
            expected_value_type: vtype,
            expected_unit: match name {
                "population" => Some(Unit::Count),
                "area" => Some(Unit::SquareKilometer),
                _ => None,
            },
            aliases: aliases.into_iter().map(String::from).collect(),
            domain: DomainId(1),
            verification_threshold: match name {
                "population" => Some(VerificationThreshold {
                    numeric_tolerance: Some(0.05),
                    date_tolerance_days: None,
                    coordinate_tolerance_km: None,
                }),
                _ => None,
            },
        });
    }
}

/// Entity ID tracking — maps Wikidata QID to our EntityId.
struct IdMap {
    map: HashMap<String, EntityId>,
    next_id: u64,
}

impl IdMap {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            next_id: 1,
        }
    }

    fn get_or_create(&mut self, qid: &str) -> EntityId {
        if let Some(&id) = self.map.get(qid) {
            return id;
        }
        let id = EntityId(self.next_id);
        self.next_id += 1;
        self.map.insert(qid.to_string(), id);
        id
    }
}

// We use a static mut for simplicity in this import tool.
// In production, this would be passed around properly.
static mut NEXT_RELATION_ID: u64 = 1;

fn next_relation_id() -> RelationId {
    unsafe {
        let id = RelationId(NEXT_RELATION_ID);
        NEXT_RELATION_ID += 1;
        id
    }
}

fn wikidata_source() -> SourceRef {
    SourceRef {
        kind: SourceKind::KnowledgeBase,
        name: "Wikidata".to_string(),
        locator: "https://www.wikidata.org".to_string(),
        date: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        ),
    }
}

fn extract_qid(uri: &str) -> Option<String> {
    uri.rsplit('/').next().map(|s| s.to_string())
}

async fn sparql_query(
    client: &reqwest::Client,
    query: &str,
) -> Result<serde_json::Value> {
    let resp = client
        .get(WIKIDATA_SPARQL)
        .query(&[("query", query), ("format", "json")])
        .send()
        .await
        .map_err(|e| ParallaxisError::Vault(format!("SPARQL request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ParallaxisError::Vault(format!(
            "SPARQL returned {}: {}",
            status, &body[..body.len().min(500)]
        )));
    }

    resp.json()
        .await
        .map_err(|e| ParallaxisError::Vault(format!("SPARQL JSON parse failed: {}", e)))
}

async fn fetch_countries(client: &reqwest::Client, vault: &mut Vault) -> Result<()> {
    let query = r#"
SELECT ?country ?countryLabel ?capital ?capitalLabel ?population ?area ?continent ?continentLabel ?coord WHERE {
  ?country wdt:P31 wd:Q3624078.
  OPTIONAL { ?country wdt:P36 ?capital. }
  OPTIONAL { ?country wdt:P1082 ?population. }
  OPTIONAL { ?country wdt:P2046 ?area. }
  OPTIONAL { ?country wdt:P30 ?continent. }
  OPTIONAL { ?country wdt:P625 ?coord. }
  SERVICE wikibase:label { bd:serviceParam wikibase:language "en,pt". }
}
"#;

    let data = sparql_query(client, query).await?;
    let bindings = data["results"]["bindings"]
        .as_array()
        .ok_or_else(|| ParallaxisError::Vault("No bindings in SPARQL response".into()))?;

    info!("Got {} country rows from Wikidata", bindings.len());

    let mut id_map = IdMap::new();
    let mut seen_countries: HashMap<String, bool> = HashMap::new();
    let source = wikidata_source();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    for row in bindings {
        let country_uri = match row["country"]["value"].as_str() {
            Some(u) => u,
            None => continue,
        };
        let country_qid = match extract_qid(country_uri) {
            Some(q) => q,
            None => continue,
        };

        let country_label_en = row["countryLabel"]["value"].as_str().unwrap_or("Unknown");

        let country_id = id_map.get_or_create(&country_qid);

        // Add entity (only once)
        if !seen_countries.contains_key(&country_qid) {
            let labels = vec![Label {
                text: country_label_en.to_string(),
                language: "en".to_string(),
                primary: true,
            }];

            vault.add_entity(Entity {
                id: country_id,
                kind: EntityKind {
                    id: 1,
                    name: "country".to_string(),
                    parent: None,
                },
                labels,
                domain: DomainId(1),
            });
            seen_countries.insert(country_qid.clone(), true);
        }

        // Capital
        if let Some(capital_uri) = row["capital"]["value"].as_str() {
            if let Some(capital_qid) = extract_qid(capital_uri) {
                let capital_id = id_map.get_or_create(&capital_qid);
                let capital_label = row["capitalLabel"]["value"].as_str().unwrap_or("Unknown");

                // Add capital entity if not exists
                if vault.get_entity(capital_id).is_none() {
                    vault.add_entity(Entity {
                        id: capital_id,
                        kind: EntityKind {
                            id: 2,
                            name: "city".to_string(),
                            parent: None,
                        },
                        labels: vec![Label {
                            text: capital_label.to_string(),
                            language: "en".to_string(),
                            primary: true,
                        }],
                        domain: DomainId(1),
                    });
                }

                vault.add_relation(Relation {
                    id: next_relation_id(),
                    subject: country_id,
                    predicate: PredicateId(1), // capital
                    value: Value::Entity(capital_id),
                    confidence: Confidence::Verified,
                    source: source.clone(),
                    domain: DomainId(1),
                    valid_from: None,
                    valid_until: None,
                    timestamp: now,
                });
            }
        }

        // Population
        if let Some(pop_str) = row["population"]["value"].as_str() {
            if let Ok(pop) = pop_str.parse::<f64>() {
                vault.add_relation(Relation {
                    id: next_relation_id(),
                    subject: country_id,
                    predicate: PredicateId(2), // population
                    value: Value::Number {
                        value: pop,
                        unit: Unit::Count,
                    },
                    confidence: Confidence::Attested,
                    source: source.clone(),
                    domain: DomainId(1),
                    valid_from: None,
                    valid_until: None,
                    timestamp: now,
                });
            }
        }

        // Area
        if let Some(area_str) = row["area"]["value"].as_str() {
            if let Ok(area) = area_str.parse::<f64>() {
                vault.add_relation(Relation {
                    id: next_relation_id(),
                    subject: country_id,
                    predicate: PredicateId(3), // area
                    value: Value::Number {
                        value: area,
                        unit: Unit::SquareKilometer,
                    },
                    confidence: Confidence::Attested,
                    source: source.clone(),
                    domain: DomainId(1),
                    valid_from: None,
                    valid_until: None,
                    timestamp: now,
                });
            }
        }

        // Continent
        if let Some(cont_uri) = row["continent"]["value"].as_str() {
            if let Some(cont_qid) = extract_qid(cont_uri) {
                let cont_id = id_map.get_or_create(&cont_qid);
                let cont_label = row["continentLabel"]["value"].as_str().unwrap_or("Unknown");

                if vault.get_entity(cont_id).is_none() {
                    vault.add_entity(Entity {
                        id: cont_id,
                        kind: EntityKind {
                            id: 3,
                            name: "continent".to_string(),
                            parent: None,
                        },
                        labels: vec![Label {
                            text: cont_label.to_string(),
                            language: "en".to_string(),
                            primary: true,
                        }],
                        domain: DomainId(1),
                    });
                }

                vault.add_relation(Relation {
                    id: next_relation_id(),
                    subject: country_id,
                    predicate: PredicateId(4), // continent
                    value: Value::Entity(cont_id),
                    confidence: Confidence::Verified,
                    source: source.clone(),
                    domain: DomainId(1),
                    valid_from: None,
                    valid_until: None,
                    timestamp: now,
                });
            }
        }

        // Language
        if let Some(lang_uri) = row["language"]["value"].as_str() {
            if let Some(lang_qid) = extract_qid(lang_uri) {
                let lang_id = id_map.get_or_create(&lang_qid);
                let lang_label = row["languageLabel"]["value"].as_str().unwrap_or("Unknown");

                if vault.get_entity(lang_id).is_none() {
                    vault.add_entity(Entity {
                        id: lang_id,
                        kind: EntityKind {
                            id: 4,
                            name: "language".to_string(),
                            parent: None,
                        },
                        labels: vec![Label {
                            text: lang_label.to_string(),
                            language: "en".to_string(),
                            primary: true,
                        }],
                        domain: DomainId(1),
                    });
                }

                vault.add_relation(Relation {
                    id: next_relation_id(),
                    subject: country_id,
                    predicate: PredicateId(5), // official_language
                    value: Value::Entity(lang_id),
                    confidence: Confidence::Attested,
                    source: source.clone(),
                    domain: DomainId(1),
                    valid_from: None,
                    valid_until: None,
                    timestamp: now,
                });
            }
        }

        // Currency
        if let Some(curr_uri) = row["currency"]["value"].as_str() {
            if let Some(curr_qid) = extract_qid(curr_uri) {
                let curr_id = id_map.get_or_create(&curr_qid);
                let curr_label = row["currencyLabel"]["value"].as_str().unwrap_or("Unknown");

                if vault.get_entity(curr_id).is_none() {
                    vault.add_entity(Entity {
                        id: curr_id,
                        kind: EntityKind {
                            id: 5,
                            name: "currency".to_string(),
                            parent: None,
                        },
                        labels: vec![Label {
                            text: curr_label.to_string(),
                            language: "en".to_string(),
                            primary: true,
                        }],
                        domain: DomainId(1),
                    });
                }

                vault.add_relation(Relation {
                    id: next_relation_id(),
                    subject: country_id,
                    predicate: PredicateId(6), // currency
                    value: Value::Entity(curr_id),
                    confidence: Confidence::Attested,
                    source: source.clone(),
                    domain: DomainId(1),
                    valid_from: None,
                    valid_until: None,
                    timestamp: now,
                });
            }
        }

        // Coordinates
        if let Some(coord_str) = row["coord"]["value"].as_str() {
            if let Some((lat, lon)) = parse_wkt_point(coord_str) {
                vault.add_relation(Relation {
                    id: next_relation_id(),
                    subject: country_id,
                    predicate: PredicateId(7), // coordinates
                    value: Value::Coordinate { lat, lon },
                    confidence: Confidence::Attested,
                    source: source.clone(),
                    domain: DomainId(1),
                    valid_from: None,
                    valid_until: None,
                    timestamp: now,
                });
            }
        }
    }

    info!(
        "Countries: {} entities, {} relations so far",
        vault.entity_count(),
        vault.relation_count()
    );

    Ok(())
}

async fn fetch_capitals(client: &reqwest::Client, vault: &mut Vault) -> Result<()> {
    // Capitals are already added as entities during country fetch.
    // This function adds population and coordinates for capitals.
    let query = r#"
SELECT ?capital ?capitalLabel ?population ?coord WHERE {
  ?country wdt:P31 wd:Q3624078.
  ?country wdt:P36 ?capital.
  OPTIONAL { ?capital wdt:P1082 ?population. }
  OPTIONAL { ?capital wdt:P625 ?coord. }
  SERVICE wikibase:label { bd:serviceParam wikibase:language "en". }
}
"#;

    let data = sparql_query(client, query).await?;
    let bindings = data["results"]["bindings"]
        .as_array()
        .ok_or_else(|| ParallaxisError::Vault("No bindings".into()))?;

    info!("Got {} capital detail rows", bindings.len());

    let source = wikidata_source();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    for row in bindings {
        let capital_label = match row["capitalLabel"]["value"].as_str() {
            Some(l) => l,
            None => continue,
        };

        // Find the entity by label
        let entity = match vault.find_entity_by_label(capital_label) {
            Some(e) => e.id,
            None => {
                warn!("Capital not found in vault: {}", capital_label);
                continue;
            }
        };

        // Population
        if let Some(pop_str) = row["population"]["value"].as_str() {
            if let Ok(pop) = pop_str.parse::<f64>() {
                vault.add_relation(Relation {
                    id: next_relation_id(),
                    subject: entity,
                    predicate: PredicateId(2), // population
                    value: Value::Number {
                        value: pop,
                        unit: Unit::Count,
                    },
                    confidence: Confidence::Attested,
                    source: source.clone(),
                    domain: DomainId(1),
                    valid_from: None,
                    valid_until: None,
                    timestamp: now,
                });
            }
        }

        // Coordinates
        if let Some(coord_str) = row["coord"]["value"].as_str() {
            if let Some((lat, lon)) = parse_wkt_point(coord_str) {
                vault.add_relation(Relation {
                    id: next_relation_id(),
                    subject: entity,
                    predicate: PredicateId(7), // coordinates
                    value: Value::Coordinate { lat, lon },
                    confidence: Confidence::Attested,
                    source: source.clone(),
                    domain: DomainId(1),
                    valid_from: None,
                    valid_until: None,
                    timestamp: now,
                });
            }
        }
    }

    Ok(())
}

async fn fetch_borders(client: &reqwest::Client, vault: &mut Vault) -> Result<()> {
    let query = r#"
SELECT ?country ?countryLabel ?border ?borderLabel WHERE {
  ?country wdt:P31 wd:Q3624078.
  ?country wdt:P47 ?border.
  ?border wdt:P31 wd:Q3624078.
  SERVICE wikibase:label { bd:serviceParam wikibase:language "en". }
}
"#;

    let data = sparql_query(client, query).await?;
    let bindings = data["results"]["bindings"]
        .as_array()
        .ok_or_else(|| ParallaxisError::Vault("No bindings".into()))?;

    info!("Got {} border rows", bindings.len());

    let source = wikidata_source();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    for row in bindings {
        let country_label = match row["countryLabel"]["value"].as_str() {
            Some(l) => l,
            None => continue,
        };
        let border_label = match row["borderLabel"]["value"].as_str() {
            Some(l) => l,
            None => continue,
        };

        let country_entity = match vault.find_entity_by_label(country_label) {
            Some(e) => e.id,
            None => continue,
        };
        let border_entity = match vault.find_entity_by_label(border_label) {
            Some(e) => e.id,
            None => continue,
        };

        vault.add_relation(Relation {
            id: next_relation_id(),
            subject: country_entity,
            predicate: PredicateId(8), // borders
            value: Value::Entity(border_entity),
            confidence: Confidence::Verified,
            source: source.clone(),
            domain: DomainId(1),
            valid_from: None,
            valid_until: None,
            timestamp: now,
        });
    }

    Ok(())
}

async fn fetch_continents(client: &reqwest::Client, vault: &mut Vault) -> Result<()> {
    // Continents are already added during country fetch.
    // This just adds coordinates for continents.
    let query = r#"
SELECT ?continent ?continentLabel ?coord WHERE {
  VALUES ?continent { wd:Q15 wd:Q18 wd:Q46 wd:Q48 wd:Q49 wd:Q51 wd:Q538 }
  OPTIONAL { ?continent wdt:P625 ?coord. }
  SERVICE wikibase:label { bd:serviceParam wikibase:language "en". }
}
"#;

    let data = sparql_query(client, query).await?;
    let bindings = data["results"]["bindings"]
        .as_array()
        .ok_or_else(|| ParallaxisError::Vault("No bindings".into()))?;

    let source = wikidata_source();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    for row in bindings {
        let label = match row["continentLabel"]["value"].as_str() {
            Some(l) => l,
            None => continue,
        };

        let entity = match vault.find_entity_by_label(label) {
            Some(e) => e.id,
            None => continue,
        };

        if let Some(coord_str) = row["coord"]["value"].as_str() {
            if let Some((lat, lon)) = parse_wkt_point(coord_str) {
                vault.add_relation(Relation {
                    id: next_relation_id(),
                    subject: entity,
                    predicate: PredicateId(7),
                    value: Value::Coordinate { lat, lon },
                    confidence: Confidence::Attested,
                    source: source.clone(),
                    domain: DomainId(1),
                    valid_from: None,
                    valid_until: None,
                    timestamp: now,
                });
            }
        }
    }

    Ok(())
}

async fn fetch_brazilian_states(client: &reqwest::Client, vault: &mut Vault) -> Result<()> {
    let query = r#"
SELECT ?state ?stateLabel ?stateLabelPt ?capital ?capitalLabel ?capitalLabelPt ?population ?area WHERE {
  ?state wdt:P31 wd:Q485258.
  OPTIONAL { ?state wdt:P36 ?capital. }
  OPTIONAL { ?state wdt:P1082 ?population. }
  OPTIONAL { ?state wdt:P2046 ?area. }
  OPTIONAL { ?state rdfs:label ?stateLabelPt FILTER(LANG(?stateLabelPt) = "pt"). }
  OPTIONAL { ?capital rdfs:label ?capitalLabelPt FILTER(LANG(?capitalLabelPt) = "pt"). }
  SERVICE wikibase:label { bd:serviceParam wikibase:language "en". }
}
"#;

    let data = sparql_query(client, query).await?;
    let bindings = data["results"]["bindings"]
        .as_array()
        .ok_or_else(|| ParallaxisError::Vault("No bindings".into()))?;

    info!("Got {} Brazilian state rows", bindings.len());

    let source = wikidata_source();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut seen = HashMap::new();

    for row in bindings {
        let state_uri = match row["state"]["value"].as_str() {
            Some(u) => u,
            None => continue,
        };
        let state_label_en = row["stateLabel"]["value"].as_str().unwrap_or("Unknown");
        let state_label_pt = row["stateLabelPt"]["value"].as_str();

        let state_qid = match extract_qid(state_uri) {
            Some(q) => q,
            None => continue,
        };

        // Create entity if not seen
        if !seen.contains_key(&state_qid) {
            let state_id = EntityId(10000 + seen.len() as u64);
            seen.insert(state_qid.clone(), state_id);

            let mut labels = vec![Label {
                text: state_label_en.to_string(),
                language: "en".to_string(),
                primary: true,
            }];
            if let Some(pt) = state_label_pt {
                labels.push(Label {
                    text: pt.to_string(),
                    language: "pt".to_string(),
                    primary: true,
                });
            }

            vault.add_entity(Entity {
                id: state_id,
                kind: EntityKind {
                    id: 6,
                    name: "state".to_string(),
                    parent: None,
                },
                labels,
                domain: DomainId(1),
            });

            // Capital
            if let Some(cap_uri) = row["capital"]["value"].as_str() {
                if let Some(_cap_qid) = extract_qid(cap_uri) {
                    let cap_label_en = row["capitalLabel"]["value"].as_str().unwrap_or("Unknown");
                    let cap_label_pt = row["capitalLabelPt"]["value"].as_str();

                    let cap_id = EntityId(20000 + seen.len() as u64);

                    if vault.find_entity_by_label(cap_label_en).is_none() {
                        let mut cap_labels = vec![Label {
                            text: cap_label_en.to_string(),
                            language: "en".to_string(),
                            primary: true,
                        }];
                        if let Some(pt) = cap_label_pt {
                            if pt.to_lowercase() != cap_label_en.to_lowercase() {
                                cap_labels.push(Label {
                                    text: pt.to_string(),
                                    language: "pt".to_string(),
                                    primary: true,
                                });
                            }
                        }
                        vault.add_entity(Entity {
                            id: cap_id,
                            kind: EntityKind {
                                id: 2,
                                name: "city".to_string(),
                                parent: None,
                            },
                            labels: cap_labels,
                            domain: DomainId(1),
                        });
                    }

                    // Find the capital entity (might already exist)
                    if let Some(cap_entity) = vault.find_entity_by_label(cap_label_en) {
                        vault.add_relation(Relation {
                            id: next_relation_id(),
                            subject: state_id,
                            predicate: PredicateId(1), // capital
                            value: Value::Entity(cap_entity.id),
                            confidence: Confidence::Verified,
                            source: source.clone(),
                            domain: DomainId(1),
                            valid_from: None,
                            valid_until: None,
                            timestamp: now,
                        });
                    }
                }
            }

            // Population
            if let Some(pop_str) = row["population"]["value"].as_str() {
                if let Ok(pop) = pop_str.parse::<f64>() {
                    vault.add_relation(Relation {
                        id: next_relation_id(),
                        subject: state_id,
                        predicate: PredicateId(2),
                        value: Value::Number { value: pop, unit: Unit::Count },
                        confidence: Confidence::Attested,
                        source: source.clone(),
                        domain: DomainId(1),
                        valid_from: None,
                        valid_until: None,
                        timestamp: now,
                    });
                }
            }

            // Area
            if let Some(area_str) = row["area"]["value"].as_str() {
                if let Ok(area) = area_str.parse::<f64>() {
                    vault.add_relation(Relation {
                        id: next_relation_id(),
                        subject: state_id,
                        predicate: PredicateId(3),
                        value: Value::Number { value: area, unit: Unit::SquareKilometer },
                        confidence: Confidence::Attested,
                        source: source.clone(),
                        domain: DomainId(1),
                        valid_from: None,
                        valid_until: None,
                        timestamp: now,
                    });
                }
            }
        }
    }

    info!("Added {} Brazilian states", seen.len());
    Ok(())
}

async fn fetch_pt_labels(client: &reqwest::Client, vault: &mut Vault) -> Result<()> {
    let query = r#"
SELECT ?country ?countryLabel ?countryLabelPt WHERE {
  ?country wdt:P31 wd:Q3624078.
  ?country rdfs:label ?countryLabelPt FILTER(LANG(?countryLabelPt) = "pt").
  SERVICE wikibase:label { bd:serviceParam wikibase:language "en". }
}
"#;

    let data = sparql_query(client, query).await?;
    let bindings = data["results"]["bindings"]
        .as_array()
        .ok_or_else(|| ParallaxisError::Vault("No bindings".into()))?;

    info!("Got {} Portuguese label rows", bindings.len());

    let mut added = 0;
    for row in bindings {
        let en_label = match row["countryLabel"]["value"].as_str() {
            Some(l) => l,
            None => continue,
        };
        let pt_label = match row["countryLabelPt"]["value"].as_str() {
            Some(l) => l,
            None => continue,
        };

        // Find entity by English label, add PT label mapping
        if let Some(entity) = vault.find_entity_by_label(en_label) {
            let entity_id = entity.id;
            vault.graph.add_label_mapping(pt_label, "pt", entity_id);
            added += 1;
        }
    }

    info!("Added {} Portuguese labels", added);
    Ok(())
}

/// Parse WKT Point format from Wikidata: "Point(lon lat)"
fn parse_wkt_point(s: &str) -> Option<(f64, f64)> {
    let s = s.trim();
    let inner = s.strip_prefix("Point(")?.strip_suffix(')')?;
    let mut parts = inner.split_whitespace();
    let lon: f64 = parts.next()?.parse().ok()?;
    let lat: f64 = parts.next()?.parse().ok()?;
    Some((lat, lon))
}
