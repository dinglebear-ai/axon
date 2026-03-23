"""mem0 REST API server — axon-patched.

Extends the upstream server/main.py with MEM0_CONFIG_PATH support:
if MEM0_CONFIG_PATH is set and the file exists, load config from JSON
instead of the hardcoded env-var defaults. All other behaviour is
identical to the upstream implementation.
"""

import json
import logging
import os
import secrets
from typing import Any, Dict, List, Optional

from dotenv import load_dotenv
from fastapi import Depends, FastAPI, HTTPException
from fastapi.responses import JSONResponse, RedirectResponse
from fastapi.security import APIKeyHeader
from pydantic import BaseModel, Field

from mem0 import Memory

logging.basicConfig(
    level=logging.INFO, format="%(asctime)s - %(levelname)s - %(message)s"
)

load_dotenv()

ADMIN_API_KEY = os.environ.get("ADMIN_API_KEY", "")
MIN_KEY_LENGTH = 16

if not ADMIN_API_KEY:
    logging.warning(
        "ADMIN_API_KEY not set - API endpoints are UNSECURED! "
        "Set ADMIN_API_KEY environment variable for production use."
    )
else:
    if len(ADMIN_API_KEY) < MIN_KEY_LENGTH:
        logging.warning(
            "ADMIN_API_KEY is shorter than %d characters - consider a longer key.",
            MIN_KEY_LENGTH,
        )
    logging.info("API key authentication enabled")

# ---------------------------------------------------------------------------
# Config loading: JSON file takes precedence over env-var defaults.
# ---------------------------------------------------------------------------

_config_path = os.environ.get("MEM0_CONFIG_PATH", "")

if _config_path and os.path.isfile(_config_path):
    logging.info("Loading mem0 config from %s", _config_path)
    with open(_config_path) as _f:
        _startup_config: Dict[str, Any] = json.load(_f)
else:
    if _config_path:
        logging.warning(
            "MEM0_CONFIG_PATH=%s not found; falling back to env-var defaults.",
            _config_path,
        )

    POSTGRES_HOST = os.environ.get("POSTGRES_HOST", "postgres")
    POSTGRES_PORT = os.environ.get("POSTGRES_PORT", "5432")
    POSTGRES_DB = os.environ.get("POSTGRES_DB", "postgres")
    POSTGRES_USER = os.environ.get("POSTGRES_USER", "postgres")
    POSTGRES_PASSWORD = os.environ.get("POSTGRES_PASSWORD", "postgres")
    POSTGRES_COLLECTION_NAME = os.environ.get("POSTGRES_COLLECTION_NAME", "memories")
    NEO4J_URI = os.environ.get("NEO4J_URI", "bolt://neo4j:7687")
    NEO4J_USERNAME = os.environ.get("NEO4J_USERNAME", "neo4j")
    NEO4J_PASSWORD = os.environ.get("NEO4J_PASSWORD", "mem0graph")
    OPENAI_API_KEY = os.environ.get("OPENAI_API_KEY")
    HISTORY_DB_PATH = os.environ.get("HISTORY_DB_PATH", "/app/history/history.db")

    _startup_config = {
        "version": "v1.1",
        "vector_store": {
            "provider": "pgvector",
            "config": {
                "host": POSTGRES_HOST,
                "port": int(POSTGRES_PORT),
                "dbname": POSTGRES_DB,
                "user": POSTGRES_USER,
                "password": POSTGRES_PASSWORD,
                "collection_name": POSTGRES_COLLECTION_NAME,
            },
        },
        "graph_store": {
            "provider": "neo4j",
            "config": {
                "url": NEO4J_URI,
                "username": NEO4J_USERNAME,
                "password": NEO4J_PASSWORD,
            },
        },
        "llm": {
            "provider": "openai",
            "config": {
                "api_key": OPENAI_API_KEY,
                "temperature": 0.2,
                "model": "gpt-4.1-nano-2025-04-14",
            },
        },
        "embedder": {
            "provider": "openai",
            "config": {"api_key": OPENAI_API_KEY, "model": "text-embedding-3-small"},
        },
        "history_db_path": HISTORY_DB_PATH,
    }

MEMORY_INSTANCE = Memory.from_config(_startup_config)

# ---------------------------------------------------------------------------
# FastAPI app
# ---------------------------------------------------------------------------

app = FastAPI(
    title="Mem0 REST APIs",
    description=(
        "A REST API for managing and searching memories for your AI Agents and Apps.\n\n"
        "## Authentication\n"
        "When the ADMIN_API_KEY environment variable is set, all endpoints require "
        "the `X-API-Key` header for authentication."
    ),
    version="1.0.0",
)

api_key_header = APIKeyHeader(name="X-API-Key", auto_error=False)


async def verify_api_key(api_key: Optional[str] = Depends(api_key_header)):
    """Validate the API key when ADMIN_API_KEY is configured. No-op otherwise."""
    if ADMIN_API_KEY:
        if api_key is None:
            raise HTTPException(
                status_code=401,
                detail="X-API-Key header is required.",
                headers={"WWW-Authenticate": "ApiKey"},
            )
        if not secrets.compare_digest(api_key, ADMIN_API_KEY):
            raise HTTPException(
                status_code=401,
                detail="Invalid API key.",
                headers={"WWW-Authenticate": "ApiKey"},
            )
    return api_key


class Message(BaseModel):
    role: str = Field(..., description="Role of the message (user or assistant).")
    content: str = Field(..., description="Message content.")


class MemoryCreate(BaseModel):
    messages: List[Message] = Field(..., description="List of messages to store.")
    user_id: Optional[str] = None
    agent_id: Optional[str] = None
    run_id: Optional[str] = None
    metadata: Optional[Dict[str, Any]] = None


class SearchRequest(BaseModel):
    query: str = Field(..., description="Search query.")
    user_id: Optional[str] = None
    run_id: Optional[str] = None
    agent_id: Optional[str] = None
    filters: Optional[Dict[str, Any]] = None


@app.post("/configure", summary="Configure Mem0")
def set_config(
    config: Dict[str, Any], _api_key: Optional[str] = Depends(verify_api_key)
):
    """Replace the running Memory instance with a new config."""
    global MEMORY_INSTANCE
    MEMORY_INSTANCE = Memory.from_config(config)
    return {"message": "Configuration set successfully"}


@app.post("/memories", summary="Create memories")
def add_memory(
    memory_create: MemoryCreate, _api_key: Optional[str] = Depends(verify_api_key)
):
    """Store new memories."""
    if not any([memory_create.user_id, memory_create.agent_id, memory_create.run_id]):
        raise HTTPException(
            status_code=400,
            detail="At least one identifier (user_id, agent_id, run_id) is required.",
        )
    params = {
        k: v
        for k, v in memory_create.model_dump().items()
        if v is not None and k != "messages"
    }
    try:
        response = MEMORY_INSTANCE.add(
            messages=[m.model_dump() for m in memory_create.messages], **params
        )
        return JSONResponse(content=response)
    except Exception as e:
        logging.exception("Error in add_memory:")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/memories", summary="Get memories")
def get_all_memories(
    user_id: Optional[str] = None,
    run_id: Optional[str] = None,
    agent_id: Optional[str] = None,
    _api_key: Optional[str] = Depends(verify_api_key),
):
    """Retrieve stored memories."""
    if not any([user_id, run_id, agent_id]):
        raise HTTPException(
            status_code=400, detail="At least one identifier is required."
        )
    params = {
        k: v
        for k, v in {"user_id": user_id, "run_id": run_id, "agent_id": agent_id}.items()
        if v is not None
    }
    try:
        return MEMORY_INSTANCE.get_all(**params)
    except Exception as e:
        logging.exception("Error in get_all_memories:")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/memories/{memory_id}", summary="Get a memory")
def get_memory(memory_id: str, _api_key: Optional[str] = Depends(verify_api_key)):
    """Retrieve a specific memory by ID."""
    try:
        return MEMORY_INSTANCE.get(memory_id)
    except Exception as e:
        logging.exception("Error in get_memory:")
        raise HTTPException(status_code=500, detail=str(e))


@app.post("/search", summary="Search memories")
def search_memories(
    search_req: SearchRequest, _api_key: Optional[str] = Depends(verify_api_key)
):
    """Search for memories based on a query."""
    params = {
        k: v
        for k, v in search_req.model_dump().items()
        if v is not None and k != "query"
    }
    try:
        return MEMORY_INSTANCE.search(query=search_req.query, **params)
    except Exception as e:
        logging.exception("Error in search_memories:")
        raise HTTPException(status_code=500, detail=str(e))


@app.put("/memories/{memory_id}", summary="Update a memory")
def update_memory(
    memory_id: str,
    updated_memory: Dict[str, Any],
    _api_key: Optional[str] = Depends(verify_api_key),
):
    """Update an existing memory."""
    try:
        return MEMORY_INSTANCE.update(memory_id=memory_id, data=updated_memory)
    except Exception as e:
        logging.exception("Error in update_memory:")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/memories/{memory_id}/history", summary="Get memory history")
def memory_history(memory_id: str, _api_key: Optional[str] = Depends(verify_api_key)):
    """Retrieve history for a memory."""
    try:
        return MEMORY_INSTANCE.history(memory_id=memory_id)
    except Exception as e:
        logging.exception("Error in memory_history:")
        raise HTTPException(status_code=500, detail=str(e))


@app.delete("/memories/{memory_id}", summary="Delete a memory")
def delete_memory(memory_id: str, _api_key: Optional[str] = Depends(verify_api_key)):
    """Delete a specific memory by ID."""
    try:
        MEMORY_INSTANCE.delete(memory_id=memory_id)
        return {"message": "Memory deleted successfully"}
    except Exception as e:
        logging.exception("Error in delete_memory:")
        raise HTTPException(status_code=500, detail=str(e))


@app.delete("/memories", summary="Delete all memories")
def delete_all_memories(
    user_id: Optional[str] = None,
    run_id: Optional[str] = None,
    agent_id: Optional[str] = None,
    _api_key: Optional[str] = Depends(verify_api_key),
):
    """Delete all memories for a given identifier."""
    if not any([user_id, run_id, agent_id]):
        raise HTTPException(
            status_code=400, detail="At least one identifier is required."
        )
    params = {
        k: v
        for k, v in {"user_id": user_id, "run_id": run_id, "agent_id": agent_id}.items()
        if v is not None
    }
    try:
        MEMORY_INSTANCE.delete_all(**params)
        return {"message": "All relevant memories deleted"}
    except Exception as e:
        logging.exception("Error in delete_all_memories:")
        raise HTTPException(status_code=500, detail=str(e))


@app.post("/reset", summary="Reset all memories")
def reset_memory(_api_key: Optional[str] = Depends(verify_api_key)):
    """Completely reset stored memories."""
    try:
        MEMORY_INSTANCE.reset()
        return {"message": "All memories reset"}
    except Exception as e:
        logging.exception("Error in reset_memory:")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/", summary="Redirect to docs", include_in_schema=False)
def home():
    """Redirect to the OpenAPI documentation."""
    return RedirectResponse(url="/docs")


# ---------------------------------------------------------------------------
# v1 API aliases — matches ngent memory client expectations
# ---------------------------------------------------------------------------


@app.post(
    "/v1/memories/search", summary="Search memories (v1)", include_in_schema=False
)
def search_memories_v1(
    search_req: SearchRequest, _api_key: Optional[str] = Depends(verify_api_key)
):
    """v1 alias for POST /search — used by the ngent memory client."""
    return search_memories(search_req, _api_key)


@app.post("/v1/memories/", summary="Create memories (v1)", include_in_schema=False)
def add_memory_v1(
    memory_create: MemoryCreate, _api_key: Optional[str] = Depends(verify_api_key)
):
    """v1 alias for POST /memories — used by the ngent memory client."""
    return add_memory(memory_create, _api_key)


@app.get("/v1/memories/", summary="Get memories (v1)", include_in_schema=False)
def get_all_memories_v1(
    user_id: Optional[str] = None,
    run_id: Optional[str] = None,
    agent_id: Optional[str] = None,
    _api_key: Optional[str] = Depends(verify_api_key),
):
    """v1 alias for GET /memories — used by the ngent memory client."""
    return get_all_memories(user_id, run_id, agent_id, _api_key)
