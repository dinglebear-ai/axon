import { useEffect, useMemo, useState } from "react";
import type { BackendProfile, ProductIdentity } from "@/lib/backendProfiles/model";
import { assertCompatibleIdentity } from "@/lib/backendProfiles/model";
import { type ExecutionLoadoutSummary, LabbyClient } from "@/lib/clients/labbyClient";
import {
  clearLoadoutSelection,
  readDelegation,
  readLoadoutSelection,
  writeDelegation,
  writeLoadoutSelection,
} from "@/lib/labby/loadoutSelection";

const MAX_LOADOUTS = 256;

export function LoadoutSelector({ profile }: { profile: BackendProfile | null }) {
  const client = useMemo(() => (profile ? new LabbyClient(profile) : null), [profile]);
  const [identity, setIdentity] = useState<ProductIdentity | null>(null);
  const [items, setItems] = useState<ExecutionLoadoutSummary[]>([]);
  const [selection, setSelection] = useState(() => readLoadoutSelection(profile));
  const [delegation, setDelegation] = useState(() =>
    selection ? (readDelegation(selection.profileId) ?? "") : "",
  );
  const [error, setError] = useState("");

  useEffect(() => {
    setSelection(readLoadoutSelection(profile));
    setIdentity(null);
    setItems([]);
    if (!client || !profile) return;
    const abort = new AbortController();
    Promise.all([client.identity(abort.signal), client.listExecutionLoadouts(abort.signal)])
      .then(([identityResult, page]) => {
        const verified = assertCompatibleIdentity(profile, identityResult.payload);
        if (!verified.capabilities.includes("loadouts"))
          throw new Error("This Labby profile does not authorize ExecutionLoadouts.");
        setIdentity(verified);
        setItems(
          page.items
            .filter(
              (item) =>
                item.effectiveRuntimeRevision !== null || item.desiredActiveRevision !== null,
            )
            .slice(0, MAX_LOADOUTS),
        );
      })
      .catch((reason) => {
        if ((reason as Error).name !== "AbortError") setError(String(reason));
      });
    return () => abort.abort();
  }, [client, profile]);

  if (!profile)
    return <span className="loadout-select-error">Add a Labby profile to select a loadout.</span>;
  return (
    <fieldset className="loadout-select" aria-label="Ask and chat loadout">
      <legend>Loadout</legend>
      <select
        aria-label="Execution loadout"
        value={selection?.loadoutId ?? ""}
        disabled={!identity}
        onChange={(event) => {
          const item = items.find((candidate) => candidate.id === event.target.value);
          if (!item || !identity) {
            clearLoadoutSelection(profile);
            setSelection(null);
            return;
          }
          setSelection(
            writeLoadoutSelection(profile, {
              integrationId: identity.server_id,
              loadoutId: item.id,
              name: item.name,
              expectedRevision:
                item.effectiveRuntimeRevision ?? item.desiredActiveRevision ?? item.draftRevision,
              mode: selection?.mode ?? "context",
            }),
          );
          setError("");
        }}
      >
        <option value="">No loadout</option>
        {items.map((item) => (
          <option key={`${item.id}:${item.draftRevision}`} value={item.id}>
            {item.name} · r
            {item.effectiveRuntimeRevision ?? item.desiredActiveRevision ?? item.draftRevision}
          </option>
        ))}
      </select>
      {selection ? (
        <>
          <select
            aria-label="Loadout run mode"
            value={selection.mode}
            onChange={(event) =>
              setSelection(
                writeLoadoutSelection(profile, {
                  integrationId: selection.integrationId,
                  loadoutId: selection.loadoutId,
                  name: selection.name,
                  expectedRevision: selection.expectedRevision,
                  mode: event.target.value === "agent" ? "agent" : "context",
                }),
              )
            }
          >
            <option value="context">Context only</option>
            <option value="agent">Tool-using agent</option>
          </select>
          <output title={`Labby ${selection.integrationId}`}>
            r{selection.expectedRevision} ·{" "}
            {selection.mode === "agent" ? "durable agent" : "no tools"}
          </output>
          {selection.mode === "agent" ? (
            <input
              aria-label="Audience-bound Labby delegation"
              type="password"
              value={delegation}
              placeholder="Delegation token (session only)"
              onChange={(event) => {
                setDelegation(event.target.value);
                try {
                  writeDelegation(profile.id, event.target.value);
                  setError("");
                } catch (reason) {
                  setError(String(reason));
                }
              }}
            />
          ) : null}
        </>
      ) : null}
      {error ? (
        <span className="loadout-select-error" role="alert">
          {error}
        </span>
      ) : null}
    </fieldset>
  );
}
