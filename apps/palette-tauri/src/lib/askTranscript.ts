import type {
  AskActivity,
  AskAgentTurn,
  AskLoadoutProvenance,
  AskSource,
  AskTurn,
} from "@/lib/runState";

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

const MAX_DISPLAY_VALUE = 240;

export function responseTurnMetadata(payload: unknown): {
  loadout?: AskLoadoutProvenance;
  agent?: AskAgentTurn;
  activities: AskActivity[];
} {
  const root = asRecord(payload);
  const body = asRecord(root?.payload) ?? root;
  const rawLoadout = asRecord(body?.loadout);
  const rawAgent = asRecord(body?.agent);
  const correlation = asRecord(rawAgent?.correlation);
  const proposal = asRecord(rawAgent?.pendingApproval);
  const loadout =
    rawLoadout && typeof rawLoadout.loadoutId === "string"
      ? {
          integrationId: bounded(rawLoadout.integrationId),
          loadoutId: bounded(rawLoadout.loadoutId),
          requestedRevision: numberValue(rawLoadout.requestedRevision),
          effectiveRevision: numberValue(rawLoadout.effectiveRevision),
          status: rawLoadout.status === "narrowed" ? ("narrowed" as const) : ("effective" as const),
          catalogGeneration: boundedOptional(rawLoadout.catalogGeneration),
          executionContextId: boundedOptional(rawLoadout.executionContextId),
          correlationId: boundedOptional(rawLoadout.correlationId),
        }
      : undefined;
  const agent =
    rawAgent && typeof rawAgent.turnId === "string"
      ? {
          turnId: bounded(rawAgent.turnId),
          status: bounded(rawAgent.status),
          pendingApproval: proposal
            ? {
                toolCallId: bounded(proposal.toolCallId),
                toolId: bounded(proposal.toolId),
                destructive: proposal.destructive === true,
                contractHash: bounded(proposal.contractHash),
                arguments: proposal.arguments,
              }
            : undefined,
        }
      : undefined;
  const activities: AskActivity[] = [];
  if (loadout)
    activities.push({
      id: `loadout:${loadout.correlationId ?? loadout.loadoutId}`,
      label: `${loadout.loadoutId} · requested r${loadout.requestedRevision} · effective r${loadout.effectiveRevision}`,
      detail:
        loadout.status === "narrowed"
          ? "Labby narrowed unavailable or unauthorized capabilities."
          : `Catalog ${loadout.catalogGeneration ?? "not reported"}`,
      kind: loadout.status === "narrowed" ? "warning" : "done",
    });
  if (agent)
    activities.push({
      id: `agent:${agent.turnId}`,
      label: `Agent turn ${agent.status}`,
      detail: `Turn ${agent.turnId} · ${bounded(correlation?.actor)} via ${bounded(correlation?.service)}`,
      kind: agent.pendingApproval ? "approval" : agent.status === "succeeded" ? "done" : "tool",
    });
  if (agent?.pendingApproval)
    activities.push({
      id: `approval:${agent.pendingApproval.toolCallId}`,
      label: `Approval required for ${agent.pendingApproval.toolId}`,
      detail: "Resume with a Labby-issued approval token, or cancel this durable turn.",
      kind: "approval",
    });
  return { loadout, agent, activities: activities.slice(0, 16) };
}

export function attachResponseMetadata(
  transcript: AskTurn[] | undefined,
  payload: unknown,
): AskTurn[] | undefined {
  if (!transcript?.length) return transcript;
  const metadata = responseTurnMetadata(payload);
  return transcript.map((turn, index) =>
    index === transcript.length - 1 && turn.role === "assistant"
      ? {
          ...turn,
          loadout: metadata.loadout,
          agent: metadata.agent,
          activities: [...(turn.activities ?? []), ...metadata.activities].slice(-32),
        }
      : turn,
  );
}

function bounded(value: unknown) {
  return typeof value === "string" ? value.slice(0, MAX_DISPLAY_VALUE) : "unknown";
}
function boundedOptional(value: unknown) {
  return typeof value === "string" ? value.slice(0, MAX_DISPLAY_VALUE) : undefined;
}
function numberValue(value: unknown) {
  return typeof value === "number" && Number.isSafeInteger(value) ? value : 0;
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
  const scoreValue = record.score ?? record.relevance ?? record.similarity;
  const score = typeof scoreValue === "number" ? scoreValue : undefined;
  return label || url
    ? [
        {
          label: label ?? hostLabel(url ?? ""),
          url,
          title: stringValue(record.title),
          snippet: stringValue(
            record.snippet ?? record.excerpt ?? record.description ?? record.text,
          ),
          score,
        },
      ]
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
