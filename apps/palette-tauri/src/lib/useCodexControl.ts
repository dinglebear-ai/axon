import { useCallback, useEffect, useRef, useState } from "react";
import type { Client } from "./axonClient";
import {
  type CodexEvent,
  type CodexOperation,
  type CodexSnapshot,
  readCodexEvents,
  readCodexOperations,
  readCodexSnapshot,
} from "./codexControl";

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
      const [eventsResult, operationsResult] = await Promise.allSettled([
        readCodexEvents(client, cursor.current),
        readCodexOperations(client),
      ]);
      if (generation.current === current) {
        if (operationsResult.status === "fulfilled") setOperations(operationsResult.value);
        let nextEvents: CodexEvent[] | null =
          eventsResult.status === "fulfilled" ? eventsResult.value : null;
        if (
          eventsResult.status === "rejected" &&
          /boot|cursor/i.test(String(eventsResult.reason))
        ) {
          cursor.current = undefined;
          setEvents([]);
          nextEvents = await readCodexEvents(client, undefined);
        }
        const snapshotResult = await Promise.resolve(readCodexSnapshot(client)).then(
          (value) => ({ status: "fulfilled" as const, value }),
          (reason) => ({ status: "rejected" as const, reason }),
        );
        if (generation.current !== current) return;
        if (snapshotResult.status === "fulfilled") setSnapshot(snapshotResult.value);
        if (nextEvents || snapshotResult.status === "fulfilled") {
          const receivedEvents = nextEvents ?? [];
          setEvents((previous) => {
            if (
              receivedEvents.length > 0 &&
              previous.some((event) => event.cursor.boot_id !== receivedEvents[0].cursor.boot_id)
            )
              previous = [];
            const merged = new Map(
              previous.map((event) => [`${event.cursor.boot_id}:${event.cursor.sequence}`, event]),
            );
            for (const event of receivedEvents.filter(
              (event) => event.event.kind !== "server_request",
            ))
              merged.set(`${event.cursor.boot_id}:${event.cursor.sequence}`, event);
            if (snapshotResult.status === "fulfilled") {
              for (const event of snapshotResult.value.pending_server_requests ?? [])
                merged.set(`${event.cursor.boot_id}:${event.cursor.sequence}`, event);
              const pending = new Set(
                (snapshotResult.value.pending_server_requests ?? []).map(
                  (event) => `${event.cursor.boot_id}:${event.cursor.sequence}`,
                ),
              );
              for (const [key, event] of merged)
                if (event.event.kind === "server_request" && !pending.has(key)) merged.delete(key);
            }
            return [...merged.values()].slice(-100);
          });
          const last = nextEvents?.at(-1);
          if (last) cursor.current = last.cursor;
        }
        const eventFailure =
          eventsResult.status === "rejected" && !/boot|cursor/i.test(String(eventsResult.reason))
            ? eventsResult
            : undefined;
        const failure = [
          snapshotResult,
          operationsResult,
          ...(eventFailure ? [eventFailure] : []),
        ].find((result) => result.status === "rejected");
        if (failure?.status === "rejected")
          setError(
            failure.reason instanceof Error ? failure.reason.message : String(failure.reason),
          );
      }
    } catch (cause) {
      if (generation.current === current)
        setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (generation.current === current) setLoading(false);
    }
  }, [client]);
  const dismissEvent = useCallback((event: CodexEvent) => {
    setEvents((previous) =>
      previous.filter(
        (item) =>
          item.cursor.boot_id !== event.cursor.boot_id ||
          item.event.request_id !== event.event.request_id,
      ),
    );
  }, []);
  useEffect(() => {
    if (active) void refresh();
    return () => {
      generation.current += 1;
    };
  }, [active, refresh]);
  return { snapshot, events, operations, error, loading, refresh, dismissEvent };
}
