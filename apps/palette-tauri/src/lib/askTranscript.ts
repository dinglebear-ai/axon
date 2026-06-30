import type { AskActivity, AskSource, AskTurn } from "@/lib/runState";

type RecordLike = Record<string, unknown>;

export function appendAskPendingTurn(
  previous: AskTurn[] | undefined,
  prompt: string,
  id: string,
): AskTurn[] {
  return [
    ...(previous ?? []),
    { id: `${id}:user`, role: "user", content: prompt },
    { id: `${id}:assistant`, role: "assistant", content: "", pending: true },
  ];
}

export function completeLastAssistantTurn(
  transcript: AskTurn[] | undefined,
  content: string,
  sources: AskSource[] = [],
): AskTurn[] | undefined {
  if (!transcript?.length) return undefined;
  const next = [...transcript];
  for (let index = next.length - 1; index >= 0; index -= 1) {
    if (next[index]?.role === "assistant") {
      next[index] = { ...next[index], content, pending: false, sources };
      return next;
    }
  }
  return next;
}

export function completeAssistantTurnById(
  transcript: AskTurn[] | undefined,
  assistantId: string,
  content: string,
  sources: AskSource[] = [],
): AskTurn[] | undefined {
  if (!transcript?.length) return undefined;
  return transcript.map((turn) =>
    turn.id === assistantId && turn.role === "assistant"
      ? { ...turn, content, pending: false, sources }
      : turn,
  );
}

export function updateLastAssistantTurn(
  transcript: AskTurn[] | undefined,
  content: string,
): AskTurn[] | undefined {
  if (!transcript?.length) return undefined;
  const next = [...transcript];
  for (let index = next.length - 1; index >= 0; index -= 1) {
    if (next[index]?.role === "assistant") {
      next[index] = { ...next[index], content };
      return next;
    }
  }
  return next;
}

export function appendAskActivity(
  transcript: AskTurn[] | undefined,
  activity: Omit<AskActivity, "id"> & { id?: string },
): AskTurn[] | undefined {
  if (!transcript?.length) return undefined;
  const next = [...transcript];
  for (let index = next.length - 1; index >= 0; index -= 1) {
    if (next[index]?.role === "assistant") {
      const id = activity.id ?? `activity:${Date.now()}:${next[index].activities?.length ?? 0}`;
      const activities = [...(next[index].activities ?? []), { ...activity, id }];
      next[index] = { ...next[index], activities };
      return next;
    }
  }
  return next;
}

export function answerParts(
  answer: string,
  payload?: unknown,
): { answer: string; sources: AskSource[] } {
  const split = splitInlineSources(answer);
  const sources = [...sourcesFromPayload(payload), ...split.sources];
  return { answer: split.answer, sources: dedupeSources(sources) };
}

function splitInlineSources(answer: string): { answer: string; sources: AskSource[] } {
  const match = /\n+\s*(?:#{1,3}\s*)?Sources\s*:?\s*\n+([\s\S]+)$/i.exec(answer);
  if (!match?.index) return { answer, sources: [] };
  return {
    answer: answer.slice(0, match.index).trimEnd(),
    sources: parseSourceLines(match[1] ?? ""),
  };
}

function sourcesFromPayload(payload: unknown): AskSource[] {
  const record = asRecord(payload);
  if (!record) return [];
  const nested = asRecord(record.payload);
  const body = nested ?? record;
  for (const key of ["citations", "sources", "source_urls", "urls"]) {
    const value = body[key];
    if (Array.isArray(value)) return value.flatMap(sourceFromUnknown);
  }
  return [];
}

function parseSourceLines(value: string): AskSource[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .flatMap((line) => sourceFromUnknown(line.replace(/^[-*\d.)\s]+/, "")));
}

function sourceFromUnknown(value: unknown): AskSource[] {
  if (typeof value === "string") {
    const markdown = /^\[([^\]]+)\]\(([^)]+)\)/.exec(value);
    if (markdown) return [{ label: markdown[1] || hostLabel(markdown[2]), url: markdown[2] }];
    const url = /(https?:\/\/\S+)/.exec(value)?.[1]?.replace(/[),.;]+$/, "");
    if (url) return [{ label: hostLabel(url), url }];
    return value ? [{ label: value }] : [];
  }
  const record = asRecord(value);
  if (!record) return [];
  const url = stringValue(record.url ?? record.href ?? record.source_url);
  const label =
    stringValue(record.label ?? record.title ?? record.name ?? record.src ?? record.source) ??
    (url ? hostLabel(url) : undefined);
  return label || url
    ? [{ label: label ?? hostLabel(url ?? ""), url, title: stringValue(record.title) }]
    : [];
}

function dedupeSources(sources: AskSource[]): AskSource[] {
  const seen = new Set<string>();
  return sources.filter((source) => {
    const key = source.url ?? source.label;
    if (!key || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function asRecord(value: unknown): RecordLike | null {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as RecordLike) : null;
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function hostLabel(value: string): string {
  try {
    return new URL(value).hostname.replace(/^www\./, "");
  } catch {
    return value;
  }
}
