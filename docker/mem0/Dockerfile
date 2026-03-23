FROM python:3.12-slim

WORKDIR /app

# Build deps needed for psycopg2 (pulled in by apache-age-python from the graph extra)
RUN apt-get update && apt-get install -y --no-install-recommends \
    libpq-dev gcc \
    && rm -rf /var/lib/apt/lists/*

# Install server dependencies
RUN pip install --no-cache-dir \
    fastapi==0.115.8 \
    uvicorn==0.34.0 \
    pydantic==2.10.4 \
    "mem0ai[graph,llms]>=1.0.7" \
    python-dotenv==1.0.1

EXPOSE 8000

ENV PYTHONUNBUFFERED=1

# Copy our patched entrypoint
COPY main.py .

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8000"]
