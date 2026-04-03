# Axon RAG Engine

> **Blazing fast, self-hosted RAG engine for web crawling, structured extraction, and semantic search via the Model Context Protocol.**

[![Version](https://img.shields.io/badge/version-1.3.0-blue.svg)](CHANGELOG.md)
[![Rust Version](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![FastMCP](https://img.shields.io/badge/FastMCP-Supported-brightgreen.svg)](https://github.com/jlowin/fastmcp)
[![License](https://img.shields.io/badge/license-MIT-purple.svg)](LICENSE)

---

## ✨ Overview
Axon is a powerful RAG (Retrieval-Augmented Generation) infrastructure suite built in Rust. It provides AI assistants with the ability to crawl entire websites, scrape markdown content, embed documents into vector stores, and perform hybrid semantic searches with optional graph-enhanced retrieval.

### 🎯 Key Features
| Feature | Description |
|---------|-------------|
| **Atomic Hunts** | Job-isolated crawls with zero-cost reflinked "latest" views |
| **Hybrid Search** | Combined Dense + BM42 sparse vectors for superior retrieval |
| **Source Ingestion**| Native support for GitHub, Reddit, and YouTube content |
| **Graph-Enhanced**| Optional Neo4j integration for complex relationship querying |

---

## 🎯 Claude Code Integration
Install the RAG infrastructure directly from the marketplace:

```bash
# Add the marketplace
/plugin marketplace add jmagar/claude-homelab

# Install the axon engine
/plugin install axon @jmagar-claude-homelab
```

---

## ⚙️ Configuration & Credentials
Axon requires a robust backend stack (Postgres, Redis, RabbitMQ, Qdrant).

**Location:** `~/.axon/.env`

### Required Variables
```bash
QDRANT_URL="http://127.0.0.1:53333"
TEI_URL="http://your-tei-server:52000"
AXON_PG_URL="postgres://user:pass@localhost/axon"
AXON_AMQP_URL="amqp://localhost/%2f"
```

---

## 🛠️ Available Tools & Resources

### 🔧 Primary Tool: `axon`
The unified `axon` tool orchestrates the entire RAG pipeline.

| Action | Subactions | Description |
|--------|------------|-------------|
| **`crawl`** | `start`, `status`, `cancel` | Full-site asynchronous discovery |
| **`query`** | `hybrid`, `dense`, `sparse` | Semantic search over indexed content |
| **`ask`** | `rag`, `graph` | Grounded Q&A with LLM synthesis |
| **`ingest`**| `github`, `reddit`, `youtube` | External source synchronization |

### 📊 Resources
| URI | Description | Output Format |
|-----|-------------|---------------|
| `axon://status-dashboard` | Real-time job queue monitoring | MCP App Widget |
| `axon://live/crawl/{id}` | Streaming crawl progress | Live Feed |

---

## 🏗️ Architecture & Design
Axon is built for massive scale and reliability:
- **Distributed Workers:** Decoupled job producers and consumers via RabbitMQ.
- **Monolith Guardrails:** Strict 500-line file and 80-line function limits enforced via git hooks.
- **Auto-Switch Rendering:** Intelligent fallback from fast HTTP to headless Chrome for JS-heavy sites.

---

## 🔧 Development
### Prerequisites
- Rust 1.75+
- Docker Compose (for backend infrastructure)

### Local Loop
```bash
just test-fast        # Fastest inner-loop unit tests
just test-infra       # Infrastructure-dependent integration tests
cargo build --release # Production binary build
```

### Health Check
```bash
axon doctor           # Verify all backend services are reachable
```

---

## 🐛 Troubleshooting
| Issue | Cause | Solution |
|-------|-------|----------|
| **Jobs Stuck** | Workers Down | Start consumers with `axon crawl worker` |
| **413 Payload** | TEI Batch Size | Adjust `TEI_MAX_CLIENT_BATCH_SIZE` |
| **Thin Pages** | JS Rendering | Ensure `axon-chrome` is running |

---

## 📄 License
MIT © jmagar
