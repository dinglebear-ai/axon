import { useCallback, useEffect, useRef, useState } from "react";
import type { Client } from "./axonClient";
import { readCodexSnapshot, type CodexSnapshot } from "./codexControl";

export function useCodexControl(client: Client | null, active: boolean) {
  const generation = useRef(0);
  const [snapshot, setSnapshot] = useState<CodexSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const refresh = useCallback(async () => {
    if (!client) return;
    const current = ++generation.current;
    setLoading(true);
    setError(null);
    try {
      const value = await readCodexSnapshot(client);
      if (generation.current === current) setSnapshot(value);
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
  return { snapshot, error, loading, refresh };
}
