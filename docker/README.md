# docker/
Last Modified: 2026-02-25

Container build assets and runtime supervision scripts for Axon services.

## Purpose
- Define container images used by `docker-compose.yaml`.
- Configure multi-worker supervision for the `axon-workers` container.
- Store service-specific runtime config (RabbitMQ and worker health checks).

## Layout
- `Dockerfile`: primary Axon image build used for worker/runtime containers.
- `chrome/Dockerfile`: Chrome/headless browser service image build.
- `s6/`: s6-overlay init and service definitions for worker processes.
- `scripts/healthcheck-workers.sh`: health probe script used by worker container.
- `rabbitmq/20-axon.conf`: RabbitMQ runtime configuration overlay.

## Worker Supervision (s6)
- `s6/cont-init.d/10-load-axon-env`: loads environment before services start.
- `s6/s6-rc.d/crawl-worker/run`: launches crawl worker process.
- `s6/s6-rc.d/extract-worker/run`: launches extract worker process.
- `s6/s6-rc.d/embed-worker/run`: launches embed worker process.
- `s6/s6-rc.d/ingest-worker/run`: launches ingest worker process.
- `s6/s6-rc.d/graph-worker/run`: launches graph worker process.
- `s6/s6-rc.d/web-server/run`: launches axon serve (HTTP + WebSocket) process.
- `s6/s6-rc.d/user/contents.d/*`: composes enabled worker services.

## Integration Points
- `docker-compose.yaml` references these Dockerfiles and scripts.
- Worker services execute `axon <job> worker` commands that consume RabbitMQ jobs and update Postgres job state.
- Chrome image supports render-mode flows that require browser/CDP execution.

## Operational Notes
- Build from repository root so Docker context includes source required by `docker/Dockerfile`.
- Keep worker scripts aligned with CLI worker subcommands and queue names.
- Any worker lifecycle changes should be mirrored in healthcheck and s6 service definitions.
