FROM python:3.12-slim

WORKDIR /app

# Install server dependencies.
# We explicitly install the Neo4j graph extras instead of mem0ai[graph]
# to avoid apache-age-python, which requires psycopg2 source compilation.
# We also install mem0ai[llms] for Ollama support.
RUN pip install --no-cache-dir \
    fastapi==0.115.8 \
    uvicorn==0.34.0 \
    pydantic==2.10.4 \
    "mem0ai[llms]>=1.0.7" \
    langchain-neo4j \
    neo4j \
    rank-bm25 \
    python-dotenv==1.0.1

EXPOSE 8000

ENV PYTHONUNBUFFERED=1

# Copy our patched entrypoint
COPY main.py .

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8000"]
