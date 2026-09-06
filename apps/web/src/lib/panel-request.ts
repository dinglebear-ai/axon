export class PanelRequestError extends Error {
  readonly status?: number;

  constructor(message: string, status?: number) {
    super(message);
    this.name = 'PanelRequestError';
    this.status = status;
  }
}

export async function requestPanelJson<T>(
  input: RequestInfo | URL,
  init: RequestInit = {},
  fetcher: typeof fetch = fetch
): Promise<T> {
  let response: Response;
  try {
    response = await fetcher(input, init);
  } catch (error) {
    throw new PanelRequestError(error instanceof Error ? error.message : String(error));
  }

  const text = await response.text();
  if (!response.ok) {
    throw new PanelRequestError(text || `Request failed with HTTP ${response.status}`, response.status);
  }
  try {
    return JSON.parse(text) as T;
  } catch {
    throw new PanelRequestError(`Request returned invalid JSON (HTTP ${response.status})`, response.status);
  }
}
