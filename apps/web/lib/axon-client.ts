import type { components } from './generated/axon-api';

type FetchLike = typeof fetch;

export type JsonObject = Record<string, unknown>;
export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonObject
  | JsonValue[];

export type AxonClientOptions = {
  baseUrl?: string;
  token?: string;
  headers?: HeadersInit;
  fetch?: FetchLike;
};

type Schemas = components['schemas'];
type ErrorBodySchema = Schemas['ErrorBody'];
type RestQueryRequestSchema = Schemas['RestQueryRequest'];
type RestRetrieveRequestSchema = Schemas['RestRetrieveRequest'];
type RestAskRequestSchema = Schemas['RestAskRequest'];
type RestEvaluateRequestSchema = Schemas['RestEvaluateRequest'];
type RestSuggestRequestSchema = Schemas['RestSuggestRequest'];
type RestScrapeRequestSchema = Schemas['RestScrapeRequest'];
type RestMapRequestSchema = Schemas['RestMapRequest'];
type RestSearchRequestSchema = Schemas['RestSearchRequest'];
type RestResearchRequestSchema = Schemas['RestResearchRequest'];
type RestCrawlRequestSchema = Schemas['RestCrawlRequest'];
type RestEmbedRequestSchema = Schemas['RestEmbedRequest'];
type RestExtractRequestSchema = Schemas['RestExtractRequest'];
type RestIngestRequestSchema = Schemas['RestIngestRequest'];
type AcceptedJobSchema = Schemas['AcceptedJob'];
type WatchCreateRequestSchema = Schemas['WatchDefCreateRequest'];

export interface ErrorBody extends ErrorBodySchema {}

export class AxonApiError extends Error {
  readonly status: number;
  readonly body: unknown;

  constructor(status: number, body: unknown) {
    const message =
      isErrorBody(body) && body.message ? body.message : `Axon API request failed with HTTP ${status}`;
    super(message);
    this.name = 'AxonApiError';
    this.status = status;
    this.body = body;
  }
}

export type PaginationParams = {
  limit?: number;
  offset?: number;
};

export interface QueryRequest extends RestQueryRequestSchema {}
export interface RetrieveRequest extends RestRetrieveRequestSchema {}
export interface AskRequest extends RestAskRequestSchema {}
export interface EvaluateRequest extends RestEvaluateRequestSchema {}
export interface SuggestRequest extends RestSuggestRequestSchema {}
export interface ScrapeRequest extends RestScrapeRequestSchema {}
export interface MapRequest extends RestMapRequestSchema {}
export interface SearchRequest extends RestSearchRequestSchema {}
export interface ResearchRequest extends RestResearchRequestSchema {}
export interface CrawlStartRequest extends RestCrawlRequestSchema {}
export interface EmbedStartRequest extends RestEmbedRequestSchema {}
export interface ExtractStartRequest extends RestExtractRequestSchema {}
export interface IngestStartRequest extends RestIngestRequestSchema {}
export interface AcceptedJob extends AcceptedJobSchema {}
export interface WatchCreateRequest extends WatchCreateRequestSchema {}

export type JobKind = 'crawl' | 'embed' | 'extract' | 'ingest';

export class AxonClient {
  private readonly baseUrl: string;
  private readonly token?: string;
  private readonly defaultHeaders?: HeadersInit;
  private readonly fetchImpl: FetchLike;

  constructor(options: AxonClientOptions = {}) {
    this.baseUrl = normalizeBaseUrl(options.baseUrl ?? '');
    this.token = options.token;
    this.defaultHeaders = options.headers;
    this.fetchImpl = options.fetch ?? fetch;
  }

  sources(params?: PaginationParams): Promise<unknown> {
    return this.get('/v1/sources', params);
  }

  domains(params?: PaginationParams): Promise<unknown> {
    return this.get('/v1/domains', params);
  }

  stats(): Promise<unknown> {
    return this.get('/v1/stats');
  }

  status(): Promise<unknown> {
    return this.get('/v1/status');
  }

  doctor(): Promise<unknown> {
    return this.get('/v1/doctor');
  }

  query(body: QueryRequest): Promise<unknown> {
    return this.post('/v1/query', body);
  }

  retrieve(body: RetrieveRequest): Promise<unknown> {
    return this.post('/v1/retrieve', body);
  }

  ask(body: AskRequest): Promise<unknown> {
    return this.post('/v1/ask', body);
  }

  evaluate(body: EvaluateRequest): Promise<unknown> {
    return this.post('/v1/evaluate', body);
  }

  suggest(body: SuggestRequest = {}): Promise<unknown> {
    return this.post('/v1/suggest', body);
  }

  scrape(body: ScrapeRequest): Promise<unknown> {
    return this.post('/v1/scrape', body);
  }

  map(body: MapRequest): Promise<unknown> {
    return this.post('/v1/map', body);
  }

  search(body: SearchRequest): Promise<unknown> {
    return this.post('/v1/search', body);
  }

  research(body: ResearchRequest): Promise<unknown> {
    return this.post('/v1/research', body);
  }

  startCrawl(body: CrawlStartRequest): Promise<AcceptedJob> {
    return this.post('/v1/crawl', body);
  }

  startEmbed(body: EmbedStartRequest): Promise<AcceptedJob> {
    return this.post('/v1/embed', body);
  }

  startExtract(body: ExtractStartRequest): Promise<AcceptedJob> {
    return this.post('/v1/extract', body);
  }

  startIngest(body: IngestStartRequest): Promise<AcceptedJob> {
    return this.post('/v1/ingest', body);
  }

  listJobs(kind: JobKind, params?: PaginationParams): Promise<unknown> {
    return this.get(`/v1/${kind}`, params);
  }

  jobStatus(kind: JobKind, id: string): Promise<unknown> {
    return this.get(`/v1/${kind}/${encodeURIComponent(id)}`);
  }

  cancelJob(kind: JobKind, id: string): Promise<unknown> {
    return this.post(`/v1/${kind}/${encodeURIComponent(id)}/cancel`);
  }

  cleanupJobs(kind: JobKind): Promise<unknown> {
    return this.post(`/v1/${kind}/cleanup`);
  }

  clearJobs(kind: JobKind): Promise<unknown> {
    return this.delete(`/v1/${kind}`);
  }

  recoverJobs(kind: JobKind): Promise<unknown> {
    return this.post(`/v1/${kind}/recover`);
  }

  dedupe(): Promise<unknown> {
    return this.post('/v1/dedupe');
  }

  listWatch(params?: Pick<PaginationParams, 'limit'>): Promise<unknown> {
    return this.get('/v1/watch', params);
  }

  createWatch(body: WatchCreateRequest): Promise<unknown> {
    return this.post('/v1/watch', body);
  }

  runWatch(id: string): Promise<unknown> {
    return this.post(`/v1/watch/${encodeURIComponent(id)}/run`);
  }

  openApi(): Promise<unknown> {
    return this.get('/api-docs/openapi.json');
  }

  private get<T>(path: string, query?: Record<string, string | number | boolean | undefined>): Promise<T> {
    return this.request<T>(path, { method: 'GET', query });
  }

  private post<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>(path, { method: 'POST', body });
  }

  private delete<T>(path: string): Promise<T> {
    return this.request<T>(path, { method: 'DELETE' });
  }

  private async request<T>(
    path: string,
    options: {
      method: 'GET' | 'POST' | 'DELETE';
      query?: Record<string, string | number | boolean | undefined>;
      body?: unknown;
    },
  ): Promise<T> {
    const headers = new Headers(this.defaultHeaders);
    if (this.token) {
      headers.set('authorization', `Bearer ${this.token}`);
    }
    if (options.body !== undefined) {
      headers.set('content-type', 'application/json');
    }

    const response = await this.fetchImpl(this.url(path, options.query), {
      method: options.method,
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
    });
    const body = await readResponseBody(response);
    if (!response.ok) {
      throw new AxonApiError(response.status, body);
    }
    return body as T;
  }

  private url(path: string, query?: Record<string, string | number | boolean | undefined>): string {
    const normalizedPath = path.startsWith('/') ? path : `/${path}`;
    const url = new URL(`${this.baseUrl}${normalizedPath}`, 'http://axon.local');
    for (const [key, value] of Object.entries(query ?? {})) {
      if (value !== undefined) {
        url.searchParams.set(key, String(value));
      }
    }
    if (!this.baseUrl) {
      return `${url.pathname}${url.search}`;
    }
    return url.toString();
  }
}

async function readResponseBody(response: Response): Promise<unknown> {
  if (response.status === 204) {
    return undefined;
  }
  const text = await response.text();
  if (!text) {
    return undefined;
  }
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}

function normalizeBaseUrl(baseUrl: string): string {
  return baseUrl.endsWith('/') ? baseUrl.slice(0, -1) : baseUrl;
}

function isErrorBody(body: unknown): body is ErrorBody {
  return (
    typeof body === 'object' &&
    body !== null &&
    'message' in body &&
    typeof (body as { message: unknown }).message === 'string'
  );
}
