//! Vault persistence — V1 uses JSON, V2+ will use binary mmap.

use std::fs;
use std::path::Path;

use parallaxis_core::*;
use tracing::info;

use crate::Vault;

/// Serializable vault format (V1: JSON).
#[derive(serde::Serialize, serde::Deserialize)]
struct VaultData {
    version: String,
    domain: Domain,
    entities: Vec<Entity>,
    predicates: Vec<Predicate>,
    relations: Vec<Relation>,
}

/// Save vault to a directory as JSON files.
pub fn save_vault(vault: &Vault, path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;

    let data = VaultData {
        version: vault.version.clone(),
        domain: vault.domain.clone(),
        entities: vault.graph.all_entities().cloned().collect(),
        predicates: vault.graph.all_predicates().cloned().collect(),
        relations: vault.graph.all_relations().to_vec(),
    };

    let json = serde_json::to_string_pretty(&data)
        .map_err(|e| ParallaxisError::Serialization(e.to_string()))?;

    let file_path = path.join("vault.json");
    fs::write(&file_path, json)?;

    info!(
        "Vault saved: {} entities, {} relations → {}",
        vault.entity_count(),
        vault.relation_count(),
        file_path.display()
    );

    Ok(())
}

/// Load vault from a directory.
pub fn load_vault(path: &Path) -> Result<Vault> {
    let file_path = path.join("vault.json");
    let json = fs::read_to_string(&file_path)?;

    let data: VaultData = serde_json::from_str(&json)
        .map_err(|e| ParallaxisError::Serialization(e.to_string()))?;

    let mut vault = Vault::new(data.domain, data.version);

    for predicate in data.predicates {
        vault.add_predicate(predicate);
    }

    for entity in data.entities {
        vault.add_entity(entity);
    }

    for relation in data.relations {
        vault.add_relation(relation);
    }

    info!(
        "Vault loaded: {} entities, {} relations ← {}",
        vault.entity_count(),
        vault.relation_count(),
        file_path.display()
    );

    Ok(vault)
}
