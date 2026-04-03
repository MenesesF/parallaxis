# ⟁ Parallaxis

**Factual verification layer for LLM output.**

> "Pode não ser tão fluente. Pode não conversar tão bonito. Mas quando disser algo, você pode confiar — ou pelo menos auditar por que disse."

LLMs generate convincing text but can't guarantee accuracy. Parallaxis intercepts LLM output, decomposes it into atomic claims, and verifies each one against a curated knowledge graph (Vault). Every claim gets a confidence tag — confirmed, contradicted, imprecise, or unverifiable.

**Not another AI. A truth layer.**

## What it does

```
Your LLM responds: "Brasília is the capital of Brazil, which has
                     a population of 215 million people."

Parallaxis verifies:
  ✓ "Brasília is the capital of Brazil"     [confirmed — IBGE]
  ⚠ "population of 215 million"             [divergent — Vault: 203M (2022), data may be outdated]
```

Every fact is checked. Every source is cited. Every uncertainty is flagged.

## Quick Start

```bash
# Clone
git clone https://github.com/MenesesF/parallaxis.git
cd parallaxis

# Build (requires Rust 1.85+)
cargo build --release

# Import geography data from Wikidata
cargo run --bin vault-import

# Run the demo
cargo run --bin parallaxis -- demo

# Start the API + Playground
cargo run --bin parallaxis -- serve --vault data/geography --port 3000
# Open http://localhost:3000
```

## API

### POST /verify
Verify LLM output text against the Vault.

```bash
curl -X POST http://localhost:3000/verify \
  -H "Content-Type: application/json" \
  -d '{"text": "Paris is the capital of France", "mode": "explain"}'
```

### POST /ask
Ask the Vault directly (no LLM needed).

```bash
curl -X POST http://localhost:3000/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "What is the capital of Brazil?"}'

# Response:
# {"answer": "Brasília", "source": "Wikidata", "confidence": "Verified", "found": true}
```

Works in English and Portuguese:
```bash
curl -X POST http://localhost:3000/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "Qual é a capital da França?"}'
```

### GET /info
Vault statistics.

### GET /health
Health check.

### Transparency Headers
Every response includes:
```
X-Parallaxis-Vault: geography-v2026Q2
X-Parallaxis-Coverage: 0.87
X-Parallaxis-Latency-Extract: 12ms
X-Parallaxis-Latency-Verify: 3ms
```

## Playground

Open `http://localhost:3000` for the web playground — paste LLM text, see verification results with color-coded confidence tags.

## Verification Statuses

| Status | Meaning |
|--------|---------|
| ✓ `confirmed` | Vault confirms, source traceable |
| ✗ `contradicted` | Vault explicitly contradicts |
| ~ `imprecise` | Close but not exact (within threshold) |
| ⚠ `divergent` | Values differ, vault may be outdated |
| ⏰ `outdated` | Was true, newer data available |
| 🚫 `debunked` | Famously false, debunk available |
| ? `unverifiable` | Not in vault (not "false" — unknown) |
| 💭 `opinion` | Subjective, not factually verifiable |

## Architecture

```
parallaxis/
├── crates/
│   ├── core/           Core types (Entity, Relation, Value, Claim, Verification)
│   ├── vault/          Knowledge graph storage + indexing
│   ├── extractor/      Decomposes text into claims (LLM-powered or rule-based)
│   ├── normalizer/     Unit conversion + value comparison
│   ├── verifier/       Checks claims against vault (direct lookup + inference)
│   ├── tagger/         Produces tagged output (simple/explain modes)
│   └── protocol/       HTTP API (Axum) + Playground
├── bins/parallaxis/    CLI binary
├── tools/vault-import/ Wikidata → Vault importer
└── playground/         Web UI
```

Written in **Rust** for performance and correctness. Zero-copy where possible.

## Vault

The Vault is Parallaxis's source of truth — a curated knowledge graph.

**Current:** Geography (Wikidata CC0) — 400+ entities, 2300+ relations  
Countries, capitals, continents, borders, populations, coordinates.

**Planned:** Science, Medicine, Law (domain-specific Vaults)

## LLM Extractor

Set environment variables to enable LLM-powered claim extraction:

```bash
export PARALLAXIS_LLM_URL="https://api.openai.com/v1/chat/completions"
export PARALLAXIS_LLM_KEY="sk-..."
export PARALLAXIS_LLM_MODEL="gpt-4o-mini"
```

Works with any OpenAI-compatible API (OpenAI, Anthropic via proxy, local models).

## How it differs from...

| | Parallaxis | RAG | OpenEvidence | Guardrails AI |
|---|---|---|---|---|
| Verifies facts | ✅ per-claim | ❌ | ✅ medicine only | ❌ |
| Any LLM | ✅ | ✅ | ❌ own model | ✅ |
| Any domain | ✅ | ❌ | ❌ medicine | ❌ |
| Output tags | ✅ per-sentence | ❌ | partial | ❌ |
| Open source | ✅ AGPL-3.0 | varies | ❌ | partial |
| Confidence score | ✅ granular | ❌ | ❌ | ❌ |

## License

- **Code:** AGPL-3.0
- **Vault Data:** CC-BY-SA-4.0

## Contributing

See [GOVERNANCE.md](GOVERNANCE.md). PRs welcome.

---

*Built by [Fernando Meneses](https://github.com/MenesesF)*
