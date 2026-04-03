//! # Parallaxis Protocol
//!
//! HTTP API layer using Axum.
//! Endpoints: POST /verify, POST /ask, GET /info, GET /health

use std::sync::Arc;
use std::time::Instant;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use parallaxis_core::*;
use parallaxis_extractor::{ExtractorBackend, SimpleExtractor, llm::{LlmExtractor, LlmExtractorConfig}};
use parallaxis_tagger::{OutputMode, tag};
use parallaxis_vault::Vault;
use parallaxis_verifier::{Verifier, VerifierConfig};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tracing::info;

/// Configuration for the API server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiConfig {
    /// LLM API URL for the extractor (OpenAI-compatible)
    pub llm_api_url: Option<String>,
    /// LLM API key
    pub llm_api_key: Option<String>,
    /// LLM model name
    pub llm_model: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            llm_api_url: std::env::var("PARALLAXIS_LLM_URL").ok(),
            llm_api_key: std::env::var("PARALLAXIS_LLM_KEY").ok(),
            llm_model: std::env::var("PARALLAXIS_LLM_MODEL").ok(),
        }
    }
}

/// Shared application state.
pub struct AppState {
    pub vault: Vault,
    pub config: ApiConfig,
}

/// Create the API router.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(playground))
        .route("/health", get(health))
        .route("/info", get(vault_info))
        .route("/verify", post(verify))
        .route("/ask", post(ask))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Start the API server.
pub async fn serve(vault: Vault, port: u16) -> std::io::Result<()> {
    let config = ApiConfig::default();
    if config.llm_api_url.is_some() {
        info!("LLM Extractor enabled: {}", config.llm_api_url.as_deref().unwrap_or(""));
    } else {
        info!("LLM Extractor not configured (set PARALLAXIS_LLM_URL, PARALLAXIS_LLM_KEY, PARALLAXIS_LLM_MODEL)");
    }
    let state = Arc::new(AppState { vault, config });
    let app = create_router(state);

    let addr = format!("0.0.0.0:{}", port);
    info!("Parallaxis API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await
}

// ── Request/Response types ───────────────────────────────────

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub text: String,
    #[serde(default = "default_domain")]
    pub domain: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

fn default_domain() -> String {
    "geography".to_string()
}

#[derive(Deserialize)]
pub struct AskRequest {
    pub question: String,
    #[serde(default = "default_domain")]
    pub domain: String,
}

#[derive(Serialize)]
pub struct InfoResponse {
    pub vault_version: String,
    pub domain: String,
    pub entity_count: usize,
    pub relation_count: usize,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Serialize)]
pub struct AskResponse {
    pub question: String,
    pub answer: Option<String>,
    pub source: Option<String>,
    pub confidence: Option<String>,
    pub found: bool,
}

// ── Handlers ─────────────────────────────────────────────────

async fn playground() -> Html<&'static str> {
    Html(include_str!("../../../playground/index.html"))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn vault_info(State(state): State<Arc<AppState>>) -> Json<InfoResponse> {
    Json(InfoResponse {
        vault_version: state.vault.version.clone(),
        domain: state.vault.domain.name.clone(),
        entity_count: state.vault.entity_count(),
        relation_count: state.vault.relation_count(),
    })
}

async fn verify(
    State(state): State<Arc<AppState>>,
    Json(req): Json<VerifyRequest>,
) -> impl IntoResponse {
    let start = Instant::now();

    let mode = match req.mode.as_deref() {
        Some("explain") => OutputMode::Explain,
        _ => OutputMode::Simple,
    };

    // Extract claims — use LLM extractor if configured, fallback to simple
    let predicates: Vec<Predicate> = state.vault.graph.all_predicates().cloned().collect();

    let extraction = if let (Some(url), Some(key), Some(model)) = (
        &state.config.llm_api_url,
        &state.config.llm_api_key,
        &state.config.llm_model,
    ) {
        let llm = LlmExtractor::new(LlmExtractorConfig {
            api_url: url.clone(),
            api_key: key.clone(),
            model: model.clone(),
            max_tokens: 2000,
        });
        llm.extract(&req.text, &predicates).await
    } else {
        let simple = SimpleExtractor;
        simple.extract(&req.text, &predicates).await
    };

    let extraction = match extraction {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                HeaderMap::new(),
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let extract_time = start.elapsed();

    // Verify
    let verify_start = Instant::now();
    let verifier = Verifier::new(&state.vault, VerifierConfig::default());
    let result = verifier.verify_all(&extraction);
    let verify_time = verify_start.elapsed();

    // Tag output
    let tagged = tag(&result, &mode);

    // Build response headers
    let mut headers = HeaderMap::new();
    if let Ok(v) = state.vault.version.parse() {
        headers.insert("X-Parallaxis-Vault", v);
    }
    if let Ok(v) = format!("{:.2}", tagged.coverage).parse() {
        headers.insert("X-Parallaxis-Coverage", v);
    }
    if let Ok(v) = format!("{}ms", extract_time.as_millis()).parse() {
        headers.insert("X-Parallaxis-Latency-Extract", v);
    }
    if let Ok(v) = format!("{}ms", verify_time.as_millis()).parse() {
        headers.insert("X-Parallaxis-Latency-Verify", v);
    }

    (
        StatusCode::OK,
        headers,
        Json(serde_json::to_value(&tagged).unwrap_or_default()),
    )
}

async fn ask(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AskRequest>,
) -> Json<AskResponse> {
    // Simple pattern: try to extract entity + predicate from the question
    // "What is the capital of Brazil?" → entity="Brazil", predicate="capital"
    let question_lower = req.question.to_lowercase();

    // Try common patterns
    let result = try_parse_question(&question_lower, &state.vault);

    match result {
        Some((answer, source, confidence)) => Json(AskResponse {
            question: req.question,
            answer: Some(answer),
            source: Some(source),
            confidence: Some(format!("{:?}", confidence)),
            found: true,
        }),
        None => Json(AskResponse {
            question: req.question,
            answer: None,
            source: None,
            confidence: None,
            found: false,
        }),
    }
}

/// Try to parse a question and answer from the vault.
fn try_parse_question(question: &str, vault: &Vault) -> Option<(String, String, Confidence)> {
    // Patterns to try:
    // "what is the X of Y" / "qual é o/a X do/da Y"
    let patterns = [
        ("what is the ", " of "),
        ("what's the ", " of "),
        ("qual é a ", " do "),
        ("qual é a ", " da "),
        ("qual é o ", " do "),
        ("qual é o ", " da "),
        ("qual a ", " do "),
        ("qual a ", " da "),
        ("qual o ", " do "),
        ("qual o ", " da "),
    ];

    for (prefix, separator) in &patterns {
        if let Some(rest) = question.strip_prefix(prefix) {
            let rest = rest.trim_end_matches('?').trim();
            if let Some(sep_pos) = rest.find(separator) {
                let predicate = &rest[..sep_pos].trim();
                let entity = &rest[sep_pos + separator.len()..].trim();

                // Look up in vault
                if let Some(found_entity) = vault.find_entity_by_label(entity) {
                    if let Some(found_pred) = vault.find_predicate(predicate) {
                        let relations = vault.lookup(found_entity.id, found_pred.id);
                        if let Some(relation) = relations.first() {
                            let answer = match &relation.value {
                                Value::Entity(eid) => {
                                    vault.get_entity(*eid)
                                        .and_then(|e| e.labels.first().map(|l| l.text.clone()))
                                        .unwrap_or_else(|| format!("Entity({})", eid.0))
                                }
                                Value::Text(t) => t.clone(),
                                Value::Number { value, unit } => {
                                    format!("{} {:?}", value, unit)
                                }
                                Value::Boolean(b) => b.to_string(),
                                Value::Coordinate { lat, lon } => {
                                    format!("{}, {}", lat, lon)
                                }
                                Value::Date { timestamp, .. } => {
                                    format!("timestamp:{}", timestamp)
                                }
                                Value::List(items) => {
                                    format!("{} items", items.len())
                                }
                            };

                            let source = format!(
                                "{} ({})",
                                relation.source.name, relation.source.locator
                            );

                            return Some((answer, source, relation.confidence));
                        }
                    }
                }
            }
        }
    }

    None
}
