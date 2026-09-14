FROM python:3.14.7-slim-bookworm
RUN apt-get update \
    && apt-get install -y --no-install-recommends chromium ca-certificates iptables \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10002 browser \
    && useradd --uid 10002 --gid 10002 --no-create-home browser
WORKDIR /app
COPY scripts/e2e/gateway/ /app/gateway/
RUN chmod -R a-w /app/gateway
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
ENTRYPOINT ["python3", "/app/gateway/server.py"]
