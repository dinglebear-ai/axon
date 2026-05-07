pub const DOCKER_COMPOSE_SERVICES: &str = include_str!("../../../docker-compose.yaml");
pub const ENV_EXAMPLE: &str = include_str!("../../../.env.example");
pub const CHROME_DOCKERFILE: &str = include_str!("../../../config/chrome/Dockerfile");
pub const QDRANT_PRODUCTION_YAML: &str = include_str!("../../../config/qdrant/production.yaml");

pub const SERVICES_ENV: &str = r#"AXON_DATA_DIR=./data
TEI_HTTP_PORT=52000
TEI_EMBEDDING_MODEL=Qwen/Qwen3-Embedding-0.6B
TEI_MAX_CONCURRENT_REQUESTS=80
TEI_MAX_BATCH_TOKENS=163840
TEI_MAX_BATCH_REQUESTS=80
TEI_MAX_CLIENT_BATCH_SIZE=96
TEI_POOLING=last-token
TEI_TOKENIZATION_WORKERS=8
HF_TOKEN=
NVIDIA_VISIBLE_DEVICES=0
CUDA_VISIBLE_DEVICES=0
"#;
