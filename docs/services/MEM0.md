# axon-mem0 — Persistent AI Memory Service
Last Modified: 2026-03-23

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Docker Service](#docker-service)
4. [Configuration](#configuration)
5. [Custom Server](#custom-server)
6. [API Endpoints](#api-endpoints)
7. [Qdrant Collection](#qdrant-collection)
8. [Key Design Decisions](#key-design-decisions)
9. [Operations](#operations)
10. [Troubleshooting](#troubleshooting)
11. [Related](#related)

---

## Overview

`axon-mem0` is a self-hosted persistent memory service built on [mem0ai](https://github.com/mem0ai/mem0).
It stores and retrieves user facts across conversations, giving ngent (the ACP server) long-term memory
without relying on any cloud provider.

Memory is stored in two complementary backends:

- **Qdrant** — vector store for semantic similarity search over extracted facts
- **Neo4j** — graph store for entity relationships (e.g. `josh → builds → android-app`)

The service runs as a FastAPI container (`axon-mem0`) and is called directly by ngent on every turn.

---

## Architecture

```
ngent (Go ACP server)
    │
    ├─ POST /v1/memories/search  ──→  axon-mem0  (FastAPI :58000)
    └─ POST /v1/memories/        ──→       │
                                           ├─ LLM  (gemini-3-flash-preview via cli-api.tootie.tv)
                                           │   └─ extracts facts + entity relationships from conversation
                                           ├─ TEI  (axon-tei :80 in Docker / :52000 on host)
                                           │   └─ embeds extracted facts → 1024-dim vectors
                                           ├─ Qdrant  (axon-qdrant :6333)
                                           │   └─ stores/searches vectors  (collection: mem0)
                                           └─ Neo4j  (bolt://100.120.242.29:50211, Tailscale)
                                               └─ stores entity relationship graph
```

**Data flow — add:**

1. ngent POSTs the conversation messages + `user_id` to `/v1/memories/`
2. mem0 sends the messages to Gemini with a tool-calling prompt to extract discrete facts
3. Each fact is embedded by TEI (Qwen3-Embedding-0.6B → 1024-dim vector)
4. Facts + vectors are upserted into Qdrant (`mem0` collection)
5. Entity relationships extracted by Gemini are written to Neo4j

**Data flow — search:**

1. ngent POSTs a query string + `user_id` to `/v1/memories/search`
2. mem0 embeds the query via TEI
3. Qdrant returns the top-k nearest fact vectors for that `user_id`
4. mem0 returns the ranked fact strings to ngent
5. ngent injects the facts into the system prompt for the current turn

---

## Docker Service

Defined in `docker-compose.services.yaml`:

```yaml
axon-mem0:
  build:
    context: ./docker/mem0
    dockerfile: Dockerfile
  image: axon-mem0:latest
  container_name: axon-mem0
  ports:
    - "127.0.0.1:58000:8000"
  volumes:
    - ./docker/mem0/config.json:/app/config.json:ro
  environment:
    MEM0_CONFIG_PATH: /app/config.json
  depends_on:
    axon-qdrant: {condition: service_healthy}
    axon-tei:    {condition: service_healthy}
    axon-ollama: {condition: service_healthy}
```

The port binding is `127.0.0.1:58000` — only accessible from the host, not the network.
External access is via Tailscale on the host if needed.

### Dockerfile

- **Base image**: `python:3.12-slim`
- **Source**: `docker/mem0/Dockerfile`
- **Pinned runtime deps**:
  - `mem0ai[llms]>=1.0.7`
  - `langchain-neo4j==0.8.0`
  - `neo4j==6.1.0`
  - `rank-bm25==0.2.2`

### axon-ollama GPU reservation

`axon-ollama` has Nvidia GPU reservation added so inference does not time out:

```yaml
axon-ollama:
  environment:
    NVIDIA_VISIBLE_DEVICES: "${NVIDIA_VISIBLE_DEVICES:-0}"
    NVIDIA_REQUIRE_CUDA: "cuda>=12.2"
    CUDA_VISIBLE_DEVICES: "${CUDA_VISIBLE_DEVICES:-0}"
  deploy:
    resources:
      reservations:
        devices:
          - driver: nvidia
            count: 1
            capabilities: [gpu]
```

Even though Gemini is the primary LLM for mem0, Ollama must run on GPU to avoid timeout failures
for any other workload that calls it.

---

## Configuration

Configuration is loaded from `docker/mem0/config.json` (gitignored — contains credentials).
Use `docker/mem0/config.json.example` as the template when provisioning a new instance.

The path is injected via the `MEM0_CONFIG_PATH` environment variable.

**`config.json` structure:**

```json
{
  "llm": {
    "provider": "openai",
    "config": {
      "model": "gemini-3-flash-preview",
      "openai_base_url": "https://cli-api.tootie.tv/v1",
      "api_key": "<TOOTIE_API_KEY>",
      "temperature": 0,
      "max_tokens": 2000
    }
  },
  "embedder": {
    "provider": "openai",
    "config": {
      "model": "Qwen/Qwen3-Embedding-0.6B",
      "openai_base_url": "http://axon-tei:80/v1",
      "api_key": "none",
      "embedding_dims": 1024
    }
  },
  "vector_store": {
    "provider": "qdrant",
    "config": {
      "url": "http://axon-qdrant:6333",
      "collection_name": "mem0",
      "embedding_model_dims": 1024
    }
  },
  "graph_store": {
    "provider": "neo4j",
    "config": {
      "url": "bolt://100.120.242.29:50211",
      "username": "neo4j",
      "password": "<NEO4J_PASSWORD>"
    }
  }
}
```

**Critical config notes:**

- `vector_store.config.embedding_model_dims` **must** be `1024`. If omitted, mem0 creates the
  Qdrant collection with 1536 dims (OpenAI default), causing dimension mismatch errors on every
  write and search.
- `embedder.config.api_key` is `"none"` — TEI does not require authentication.
- The LLM uses the `openai` provider with `openai_base_url` pointing to the tootie proxy. This is
  how mem0 consumes any OpenAI-compatible API without a native provider plugin.
- The Neo4j URL (`100.120.242.29:50211`) is a Tailscale IP — Neo4j runs on a separate host in the
  homelab mesh, not in Docker Compose.

---

## Custom Server

**Source**: `docker/mem0/main.py`

A custom FastAPI server that wraps the upstream mem0 server with two additions:

1. **`MEM0_CONFIG_PATH` support** — loads config from the JSON file at startup rather than
   requiring environment variables for every setting.

2. **`/v1/` alias routes** — the upstream mem0 server uses different path prefixes; the custom
   server adds routes that match what the ngent Go client expects:

   | Route | Method | Purpose |
   |-------|--------|---------|
   | `/v1/memories/search` | POST | Search memories by query + user_id |
   | `/v1/memories/` | POST | Add new memories from conversation messages |
   | `/v1/memories/` | GET | List all memories for a user_id |

Swagger UI is available at `http://localhost:58000/docs` when the service is running.

---

## API Endpoints

### POST `/v1/memories/`

Add memories extracted from a conversation.

**Request body:**

```json
{
  "messages": [
    {"role": "user",      "content": "I use Tailscale for all my homelab networking."},
    {"role": "assistant", "content": "Noted — I'll keep that in mind."}
  ],
  "user_id": "josh"
}
```

**Do not include `agent_id`** — see [Design Decision #3](#3-agent_id-excluded-from-add-calls).

**Response:**

```json
{
  "results": [
    {"id": "...", "memory": "Uses Tailscale for homelab networking", "event": "ADD"}
  ]
}
```

An empty `results` array with no error means `agent_id` was present and triggered agent extraction
mode — user facts were silently discarded. See troubleshooting below.

---

### POST `/v1/memories/search`

Search memories relevant to a query.

**Request body:**

```json
{
  "query": "networking tools",
  "user_id": "josh"
}
```

**Response:**

```json
{
  "results": [
    {
      "id": "...",
      "memory": "Uses Tailscale for homelab networking",
      "score": 0.91
    }
  ]
}
```

---

### GET `/v1/memories/`

List all stored memories for a user.

**Query params:** `user_id=josh`

---

## Qdrant Collection

| Property | Value |
|----------|-------|
| **Name** | `mem0` |
| **Vector dims** | 1024 |
| **Distance** | Cosine |
| **Indexed payload fields** | `user_id`, `agent_id`, `run_id`, `actor_id` |
| **Dashboard** | `http://localhost:53333/dashboard` |
| **REST API** | `http://localhost:53333` |

Check collection info:

```bash
curl http://localhost:53333/collections/mem0
```

---

## Key Design Decisions

### 1. Gemini for LLM, not Ollama

mem0 requires tool-calling to extract facts and entity relationships. `qwen3.5:4b` (the primary
model in this stack) has no tool-calling support in Ollama 0.6.5.
`gemini-3-flash-preview` via the tootie OpenAI-compatible proxy handles both extraction paths:

- **Fact extraction** → upserted into Qdrant vector store
- **Entity/relationship extraction** → written to Neo4j graph store

### 2. GPU reservation on axon-ollama

`axon-ollama` originally ran on CPU. All LLM inference calls timed out (>3 min) and returned
HTTP 500. Adding `deploy.resources.reservations.devices: nvidia` to the Compose service
fixed this. Even though Gemini is the primary LLM for mem0, Ollama on GPU is required for
other workloads in the stack.

### 3. `agent_id` excluded from add calls

mem0 v1.0.7 switches to "agent memory extraction" mode (facts about the AI assistant itself)
when `agent_id` is present alongside assistant messages. In that mode, user facts are silently
discarded — `add()` returns `{"results":[]}` with no error.

The ngent memory client omits `agent_id` from all add requests so that user-facing facts
are extracted and stored correctly.

### 4. `embedding_model_dims: 1024` must be explicit

Without this field in `vector_store.config`, mem0 creates the Qdrant collection with 1536 dims
(the OpenAI `text-embedding-3-small` default). Qwen3-Embedding-0.6B outputs 1024-dim vectors,
causing a dimension mismatch error on every operation. This must be set explicitly — there is
no inference from the embedder config.

### 5. Both vector store and graph store are active

The two stores serve different purposes:

- **Qdrant** — semantic similarity search over discrete facts ("what does the user know/use?")
- **Neo4j** — entity relationship graph ("how do things connect?")

Gemini is required for both paths. TEI handles only the embedding step (fact → vector).

---

## Operations

### Start / stop

```bash
# Start mem0 and its dependencies
docker compose -f docker-compose.services.yaml up -d axon-mem0

# Stop
docker compose -f docker-compose.services.yaml stop axon-mem0
```

### Rebuild after code or Dockerfile changes

```bash
docker compose -f docker-compose.services.yaml build axon-mem0
docker compose -f docker-compose.services.yaml up -d axon-mem0
```

### Restart after config.json changes

```bash
docker compose -f docker-compose.services.yaml restart axon-mem0
```

### Health check

```bash
# Swagger UI — service is up if this returns HTML
curl -s -o /dev/null -w "%{http_code}" http://localhost:58000/docs

# Qdrant collection info
curl http://localhost:53333/collections/mem0
```

### View logs

```bash
docker logs axon-mem0 -f
```

### Manual memory add (smoke test)

```bash
curl -X POST http://localhost:58000/v1/memories/ \
  -H "Content-Type: application/json" \
  -d '{
    "messages": [
      {"role": "user",      "content": "I use Tailscale for all my homelab networking."},
      {"role": "assistant", "content": "Noted."}
    ],
    "user_id": "josh"
  }'
```

### Manual memory search (smoke test)

```bash
curl -X POST http://localhost:58000/v1/memories/search \
  -H "Content-Type: application/json" \
  -d '{"query": "networking tools", "user_id": "josh"}'
```

### Reset Qdrant collection (wipe all memories)

Only do this if there is a dimension mismatch or corrupted state:

```bash
# Delete the collection
curl -X DELETE http://localhost:53333/collections/mem0

# Restart axon-mem0 — it will recreate the collection with correct dims
docker compose -f docker-compose.services.yaml restart axon-mem0
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `add()` returns `{"results":[]}` with no error | `agent_id` in request triggers agent extraction mode — user facts are silently discarded | Omit `agent_id` from all add requests |
| Search or add hangs for >60s, then HTTP 500 | Ollama running on CPU — inference timeout | Add `deploy.resources.reservations.devices: nvidia` to `axon-ollama` in docker-compose |
| `does not support tools (status code: 400)` | Model has no tool-calling capability | Use a model with tool support (gemini, qwen2.5:7b, etc.) — not qwen3.5:4b in Ollama 0.6.5 |
| `embedding_model_dims` mismatch / Qdrant vector size error | Collection was created with 1536 dims (OpenAI default) instead of 1024 | Delete `mem0` collection in Qdrant, restart `axon-mem0` |
| `Server disconnected without sending a response` | Ollama model load/inference failure on CPU | Ensure GPU reservation is set on `axon-ollama` |
| `KeyError: entity_type` in graph store extraction | LLM returns `type` instead of `entity_type` in tool call (qwen2.5:7b quirk) | Use Gemini — it follows the mem0 tool schema correctly |
| Container fails to start | `config.json` missing or malformed | Copy `docker/mem0/config.json.example` → `docker/mem0/config.json` and fill in credentials |

---

## Related

| Resource | Description |
|----------|-------------|
| `docker/mem0/main.py` | Custom FastAPI server source |
| `docker/mem0/Dockerfile` | Container build definition |
| `docker/mem0/config.json.example` | Config template (tracked in git) |
| `docker/mem0/config.json` | Live config with credentials (gitignored) |
| `internal/memory/client.go` | ngent Go HTTP client for mem0 |
| `docs/MEMORY.md` (ngent repo) | ngent-side integration architecture |
| `services.env` | `MEM0_URL=http://localhost:58000` |
| `http://localhost:58000/docs` | Swagger UI (when running) |
| `http://localhost:53333/dashboard` | Qdrant dashboard |
