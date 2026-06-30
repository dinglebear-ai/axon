import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { ACTIONS } from "@/lib/actions";
import type { ChatSuggestion } from "@/lib/runState";
import { arrField, isRecord, numField, strField, unwrapPayload } from "@/lib/payload";
import { capHistory } from "@/lib/useActionRunner";

const HISTORY_STORAGE_KEY = "axon.palette.history.v1";
const CHAT_SUGGESTION_LIMIT = 4;
const STORED_TEXT_LIMIT = 12_000;
const STORED_TURN_LIMIT = 4_000;
const STORED_TRANSCRIPT_LIMIT = 24;

type StoredHistoryItem = Omit<HistoryItem, "action"> & { actionSubcommand: string };

export function loadPaletteHistory(): HistoryItem[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = window.localStorage.getItem(HISTORY_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.map(hydrateHistoryItem).filter((item): item is HistoryItem => Boolean(item));
  } catch {
    return [];
  }
}

export function persistPaletteHistory(items: HistoryItem[]) {
  if (typeof window === "undefined") return;
  try {
    const serialized = capHistory(items).map(serializeHistoryItem);
    try {
      window.localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(serialized));
    } catch {
      window.localStorage.setItem(
        HISTORY_STORAGE_KEY,
        JSON.stringify(serialized.slice(0, Math.max(1, Math.floor(serialized.length / 2)))),
      );
    }
  } catch {
    // History persistence is best-effort; the live palette must keep working.
  }
}

export function normalizeChatSuggestions(payload: unknown): ChatSuggestion[] {
  const data = unwrapPayload(payload);
  return arrField(data, "results")
    .flatMap((item, index): ChatSuggestion[] => {
      if (!isRecord(item)) return [];
      return [
        {
          title:
            strField(item, "title") ??
            strField(item, "name") ??
            strField(item, "url") ??
            `Result ${index + 1}`,
          url: strField(item, "url") ?? strField(item, "source_url"),
          snippet:
            strField(item, "snippet") ??
            strField(item, "content") ??
            strField(item, "text") ??
            strField(item, "reason"),
          score: numField(item, "score"),
          rank: numField(item, "rank") ?? index + 1,
        },
      ];
    })
    .slice(0, CHAT_SUGGESTION_LIMIT);
}

function serializeHistoryItem(item: HistoryItem): StoredHistoryItem {
  const { action, ...rest } = item;
  const status = rest.status || (rest.result?.ok ? 200 : rest.status);
  const transcript = rest.transcript
    ?.slice(-STORED_TRANSCRIPT_LIMIT)
    .map((turn) => ({ ...turn, content: truncateText(turn.content, STORED_TURN_LIMIT) ?? "" }));
  return {
    ...rest,
    status,
    duration: status >= 200 && status < 300 ? undefined : rest.duration,
    text: truncateText(rest.text, STORED_TEXT_LIMIT),
    transcript,
    result: rest.result ? { ...rest.result, status, payload: null } : undefined,
    actionSubcommand: action.subcommand,
  };
}

function hydrateHistoryItem(raw: unknown): HistoryItem | null {
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Partial<StoredHistoryItem>;
  const action = ACTIONS.find((candidate) => candidate.subcommand === record.actionSubcommand);
  if (!action || typeof record.target !== "string" || typeof record.status !== "number")
    return null;
  if (
    typeof record.title !== "string" ||
    typeof record.subtitle !== "string" ||
    typeof record.when !== "string"
  )
    return null;
  const { actionSubcommand: _actionSubcommand, ...rest } = record;
  return { ...(rest as Omit<HistoryItem, "action">), action };
}

function truncateText(value: string | undefined, limit: number): string | undefined {
  if (value === undefined || value.length <= limit) return value;
  return `${value.slice(0, limit)}\n\n[truncated ${value.length - limit} chars]`;
}
