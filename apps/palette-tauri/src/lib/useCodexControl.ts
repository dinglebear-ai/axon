import { useCallback, useEffect, useRef, useState } from "react";
import type { Client } from "./axonClient";
import { readCodexEvents, readCodexOperations, readCodexSnapshot, type CodexEvent, type CodexOperation, type CodexSnapshot } from "./codexControl";

export function useCodexControl(client: Client | null, active: boolean) {
  const generation = useRef(0);
  const cursor = useRef<{ boot_id: number; sequence: number } | undefined>(undefined);
  const [snapshot, setSnapshot] = useState<CodexSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [events, setEvents] = useState<CodexEvent[]>([]);
  const [operations, setOperations] = useState<CodexOperation[]>([]);
  const refresh = useCallback(async () => {
    if (!client) return;
    const current = ++generation.current;
    setLoading(true);
    setError(null);
    try {
      const [value, nextEvents, nextOperations] = await Promise.all([readCodexSnapshot(client), readCodexEvents(client, cursor.current), readCodexOperations(client)]);
      if (generation.current === current) {
        setSnapshot(value);
        setOperations(nextOperations);
        setEvents(previous => {
          const merged = new Map(previous.map(event => [`${event.cursor.boot_id}:${event.cursor.sequence}`, event]));
          for (const event of nextEvents) merged.set(`${event.cursor.boot_id}:${event.cursor.sequence}`, event);
          return [...merged.values()].slice(-100);
        });
        const last = nextEvents.at(-1);
        if (last) cursor.current = last.cursor;
      }
    } catch (cause) {
      if (generation.current === current) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (generation.current === current) setLoading(false);
    }
  }, [client]);
  useEffect(() => {
    if (active) void refresh();
    return () => { generation.current += 1; };
  }, [active, refresh]);
  return { snapshot, events, operations, error, loading, refresh };
}
