export type PanelState = {
  setup_required: boolean;
  config_path: string;
};

export type ConfigResponse = {
  path: string;
  raw_toml: string;
  restart_required: boolean;
};

export type EnvConfigResponse = {
  path: string;
  raw_env: string;
  restart_required: boolean;
};

export type SaveConfigResponse = {
  ok: boolean;
  restart_required: boolean;
  message: string;
};

export type StackCheck = {
  label: string;
  status: 'ok' | 'warn' | 'error' | 'skipped' | string;
  detail: string;
};

export type StackUrlCheck = StackCheck & {
  url: string;
};

export type StackResponse = {
  runtime_mode: 'host' | 'container' | string;
  server_url: string;
  mcp_url: string;
  log_dir: string;
  compose_file: string;
  urls: StackUrlCheck[];
  checks: StackCheck[];
};

export type PanelStatusResponse = {
  payload: {
    local_crawl_jobs?: ServiceJob[];
    local_extract_jobs?: ServiceJob[];
    local_embed_jobs?: ServiceJob[];
    local_ingest_jobs?: ServiceJob[];
    totals?: Record<string, number>;
  };
  text: string;
  totals: Record<string, number>;
};

export type ServiceJob = {
  id: string;
  status: string;
  updated_at: string;
  created_at: string;
  kind?: 'crawl' | 'extract' | 'embed' | 'ingest';
  error_text?: string | null;
  url?: string | null;
  target?: string | null;
  source_type?: string | null;
  urls_json?: unknown;
};

export type ArtifactHandle = {
  relative_path: string;
  bytes?: number;
  kind: string;
  display_path: string;
  line_count?: number;
};

export type PanelCommandResponse = {
  command: string;
  action: unknown;
  result: unknown;
};

export type CommandResultView = {
  ok: boolean;
  title: string;
  subtitle: string;
  rows: Array<{ label: string; value: string }>;
  body?: string;
  raw?: string;
  imageUrl?: string;
  imageArtifact?: ArtifactHandle;
  artifacts?: ArtifactHandle[];
};

export type PanelDoctorResponse = {
  payload: {
    observed_at_utc?: string;
    all_ok?: boolean;
    services?: Record<string, DoctorService>;
    pipelines?: Record<string, boolean>;
    browser_runtime?: {
      selection?: string;
    };
  };
};

export type DoctorService = {
  ok?: boolean;
  url?: string | null;
  detail?: string | null;
  model?: string | null;
  collection?: string | null;
  vector_mode?: string | null;
  path?: string | null;
  exists?: boolean;
  command?: string | null;
};

export type CheckSummary = {
  ok: number;
  warn: number;
  error: number;
  skipped: number;
  total: number;
};

export type ConfigFile = 'toml' | 'env';
export type PanelTab = 'dashboard' | 'jobs' | 'configurator';

export const TOKEN_KEY = 'axon-panel-token';
