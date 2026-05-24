type JsonResponse = {
  responses: {
    200: {
      content: {
        "application/json": unknown;
      };
    };
    default: {
      content: {
        "application/json": unknown;
      };
    };
  };
};

type JsonPost = JsonResponse & {
  requestBody: {
    content: {
      "application/json": Record<string, unknown>;
    };
  };
};

export interface paths {
  "/v1/doctor": { get: JsonResponse };
  "/v1/status": { get: JsonResponse };
  "/v1/sources": { get: JsonResponse };
  "/v1/domains": { get: JsonResponse };
  "/v1/stats": { get: JsonResponse };
  "/v1/scrape": { post: JsonPost };
  "/v1/crawl": { post: JsonPost };
  "/v1/map": { post: JsonPost };
  "/v1/summarize": { post: JsonPost };
  "/v1/ask": { post: JsonPost };
  "/v1/query": { post: JsonPost };
  "/v1/retrieve": { post: JsonPost };
  "/v1/suggest": { post: JsonPost };
  "/v1/evaluate": { post: JsonPost };
  "/v1/search": { post: JsonPost };
  "/v1/research": { post: JsonPost };
  "/v1/embed": { post: JsonPost };
  "/v1/extract": { post: JsonPost };
  "/v1/ingest": { post: JsonPost };
}

export interface components {
  schemas: Record<string, Record<string, unknown>>;
}
