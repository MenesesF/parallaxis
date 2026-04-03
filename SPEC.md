# Parallaxis — Specification

> "Pode não ser tão fluente. Pode não conversar tão bonito. Mas quando disser algo, você pode confiar — ou pelo menos auditar por que disse."

## O que é

Camada de verificação factual em tempo real para qualquer LLM, qualquer domínio.

LLM responde → Parallaxis intercepta cada afirmação → verifica contra base de conhecimento curado → retorna texto taggeado com nível de confiança por afirmação.

## O que NÃO é

- Não é um LLM
- Não é RAG
- Não é chatbot
- Não substitui o LLM — complementa

## Pipeline

```
Texto do LLM
     │
     ▼
 EXTRACTOR
 Decompõe texto em triplas (sujeito, predicado, objeto)
 Usa LLM pequeno com schema do Vault como contexto
 Self-check: reconstrói frase e compara com original
     │
     ▼
 NORMALIZER
 Converte unidades para padrão SI do Vault
 Resolve aliases multilíngue
     │
     ▼
 VERIFIER
 Lookup direto no Vault
 Mini-reasoner (1-3 saltos) se lookup falha
 Compara valores com thresholds por predicado
     │
     ▼
 TAGGER
 Remonta texto original com tags de verificação
 Modo simple (status) ou explain (detalhado)
     │
     ▼
 Texto taggeado com confiança
```

## Status de verificação

| Status | Significado |
|--------|------------|
| `confirmed` | Vault confirma, fonte rastreável |
| `contradicted` | Vault contradiz explicitamente |
| `imprecise` | Valor próximo mas não exato (dentro de threshold) |
| `conditional` | Verdade sob condições específicas |
| `outdated` | Era verdade, dados mais recentes diferem |
| `oversimplified` | Correto mas omite contexto importante |
| `divergent` | Valor difere e Vault pode estar desatualizado |
| `debunked` | Claim famosamente falso, refutação disponível |
| `unverifiable` | Vault não tem dados sobre isso |
| `opinion` | Afirmação subjetiva, não verificável factualmente |

## Arquitetura (Rust)

```
parallaxis/
├── Cargo.toml              (workspace)
├── crates/
│   ├── core/               tipos fundamentais, Value tipado
│   ├── vault/              grafo de conhecimento, mmap, WAL
│   ├── extractor/          decompõe texto em triplas
│   ├── normalizer/         unidades, aliases
│   ├── verifier/           checa claims contra vault + mini-reasoner
│   ├── tagger/             remonta texto com tags
│   └── protocol/           API HTTP (axum)
├── bins/
│   └── parallaxis/         binário principal
└── tools/
    └── vault-import/       importador Wikidata → Vault
```

## Vault

### Formato
- Grafo binário com mmap (zero-copy)
- Append-only com WAL (Write-Ahead Log)
- Versionado por trimestre (v2026-Q1, v2026-Q2, ...)

### Primeiro Vault: Geografia
- Fonte: Wikidata (CC0) + dados UN/IBGE
- Predicados: capital, população, área, fronteira, continente, idioma, moeda, coordenadas
- Estimativa: ~500k entidades, ~5M relações

### Tipos de valor
```rust
pub enum Value {
    Text(String),
    Number { value: f64, unit: Unit },
    Date { timestamp: i64, precision: DatePrecision },
    Boolean(bool),
    Entity(EntityId),
    Coordinate { lat: f64, lon: f64 },
}
```

### Aliases
Cada entidade pode ter múltiplos nomes em múltiplos idiomas.
"Paracetamol" = "Acetaminophen" = "パラセタモール" → mesmo EntityId.
Indexados do Wikidata.

## API

### POST /verify
```json
{
  "text": "resposta do LLM",
  "domain": "geography",
  "mode": "explain",
  "language": "pt-br"
}
```

### POST /ask
Responde direto do Vault, sem LLM.
```json
{
  "question": "Qual a capital do Brasil?",
  "domain": "geography"
}
```

### POST /compare
Compara verificação de múltiplas respostas de LLMs diferentes.

### POST /feedback
Usuário reporta verificação incorreta → alimenta Explorer.

### Headers de transparência
```
X-Parallaxis-Vault: geography-v2026Q1
X-Parallaxis-Coverage: 0.87
X-Parallaxis-Cache: HIT (age: 3d, ttl: 27d)
X-Parallaxis-Latency-Extract: 340ms
X-Parallaxis-Latency-Verify: 45ms
```

## Cache
- TTL por domínio (geografia: 180 dias, medicina: 30 dias)
- Re-verificação periódica automática
- Claims já verificados retornam instantaneamente

## Thresholds
Configuráveis por Vault e por predicado:
- População: tolerância de 3%
- Constantes físicas: tolerância de 0%
- Datas: tolerância de 0 dias
- Coordenadas: tolerância de 1km

## Produtos

### Parallaxis Core (open source)
Engine completo: extractor + verifier + tagger.
Roda local, self-hosted. Traz seu próprio Vault.

### Parallaxis API (SaaS)
REST + streaming (SSE). SDKs: Rust, Python, TypeScript.

### Parallaxis Vaults (Marketplace)
- general (Wikidata, grátis)
- Domínios pagos curados
- Community marketplace

### Parallaxis Playground (web)
Cola texto → vê verificação ao vivo. Primeira impressão do produto.

## Pricing
```
Free:       1 dev, 1k verifs/mês, Vault general
Starter:    $49/mês, 3 devs, 50k verifs/mês, 1 Vault pago
Pro:        $149/mês, 10 devs, 500k verifs/mês, todos os Vaults
Enterprise: custom, SLA, on-premise
```

## Licenças
- Código: AGPL-3.0
- Dados: CC-BY-SA-4.0
- Nome/marca: protegida

## Fases de execução

### Fase 1 (semanas 1-2): Core + Vault
- Crate `core` com tipos fundamentais
- Crate `vault` com grafo em memória + persistência binária
- Importar subset de Wikidata (geografia, ~500k entidades)

### Fase 2 (semanas 3-4): Pipeline
- Crate `extractor` (LLM extrai triplas)
- Crate `normalizer` (unidades + aliases)
- Crate `verifier` (lookup + mini-reasoner)
- Crate `tagger` (remonta texto)
- Pipeline end-to-end funcionando

### Fase 3 (semanas 5-6): Produto
- Crate `protocol` (API HTTP com axum)
- Playground web
- Landing page
- Beta fechado

### Fase 4+: Escala
- Mais Vaults (ciência, medicina, direito)
- SDKs (Python, TypeScript)
- Marketplace
- Explorer automatizado
