import { useRef, useState } from "react";
import { Button } from "@/components/ui/aurora/button";
import type { Client, PaletteConfig, PaletteHttpRequest, PaletteResult } from "@/lib/axonClient";
import { invoke } from "@/lib/invoke";
import { readDelegation } from "@/lib/labby/loadoutSelection";
import { activeProfile } from "@/lib/backendProfiles/model";
import { LabbyClient } from "@/lib/clients/labbyClient";
import type { AskAgentTurn, AskLoadoutProvenance } from "@/lib/runState";

const MAX_EVENTS = 100;
const MAX_EVENT_BYTES = 32_768;

export function AgentTurnControls({
  agent,
  loadout,
  client,
  config,
}: {
  agent: AskAgentTurn;
  loadout?: AskLoadoutProvenance;
  client: Client | null;
  config: PaletteConfig | null;
}) {
  const [status, setStatus] = useState(agent.status);
  const [events, setEvents] = useState<{ key: string; text: string }[]>([]);
  const [approving, setApproving] = useState(false);
  const approvalInFlight = useRef(false);
  const [notice, setNotice] = useState("");
  const terminal = ["succeeded", "failed", "cancelled", "timed_out"].includes(status);

  async function request(
    method: "GET" | "POST",
    path: string,
    body: Record<string, unknown> | null = null,
  ) {
    if (!client) throw new Error("Axon profile is unavailable.");
    const value = await invoke<PaletteResult>("axon_http_request", {
      request: {
        baseUrl: client.baseUrl,
        token: config?.token ?? null,
        method,
        path,
        body,
      } satisfies PaletteHttpRequest,
    });
    if (!value.ok) throw new Error(errorMessage(value.payload));
    return value.payload;
  }
  async function refresh() {
    try {
      setNotice("");
      const result = asRecord(
        await request("GET", `/v1/agent/turns/${encodeURIComponent(agent.turnId)}`),
      );
      if (typeof result.status === "string") setStatus(result.status);
      const page = asRecord(
        await request("GET", `/v1/agent/turns/${encodeURIComponent(agent.turnId)}/events`),
      );
      const raw = Array.isArray(page.items) ? page.items.slice(-MAX_EVENTS) : [];
      let bytes = 0;
      const safe = raw.flatMap((item) => {
        const text = JSON.stringify(item);
        bytes += text.length;
        const sequence = asRecord(item).sequence;
        return bytes <= MAX_EVENT_BYTES
          ? [{ key: `${String(sequence)}:${text.slice(0, 80)}`, text: text.slice(0, 2_000) }]
          : [];
      });
      setEvents(safe);
    } catch (reason) {
      setNotice(String(reason));
    }
  }
  async function cancel() {
    try {
      const result = asRecord(
        await request("POST", `/v1/agent/turns/${encodeURIComponent(agent.turnId)}/cancel`),
      );
      setStatus(typeof result.status === "string" ? result.status : "cancelled");
    } catch (reason) {
      setNotice(String(reason));
    }
  }
  async function resume() {
    if (approvalInFlight.current) return;
    approvalInFlight.current = true;
    setApproving(true);
    try {
      if (!agent.pendingApproval || !loadout || !config)
        throw new Error("Approval provenance is incomplete; reload status instead of guessing.");
      const profile = activeProfile(config.backendProfiles, config.activeBackendProfiles, "labby");
      if (!profile) throw new Error("Select the Labby profile that owns this execution context.");
      const delegationToken = profile ? readDelegation(profile.id) : null;
      if (!delegationToken)
        throw new Error("The profile-scoped Labby delegation is missing or expired.");
      if (!loadout.executionContextId)
        throw new Error("The Labby execution context is missing or stale.");
      const challenge = await new LabbyClient(profile).requestApproval({
        executionContextId: loadout.executionContextId,
        proposal: agent.pendingApproval,
      });
      validateApprovalChallenge(
        challenge,
        Date.now(),
      );
      const body = {
        approvalTokens: [
          { toolCallId: agent.pendingApproval.toolCallId, token: challenge.approvalToken },
        ],
      };
      const result = asRecord(
        await request("POST", `/v1/agent/turns/${encodeURIComponent(agent.turnId)}/resume`, body),
      );
      const next = asRecord(result.agent);
      setStatus(typeof next.status === "string" ? next.status : status);
      await refresh();
    } catch (reason) {
      setNotice(String(reason));
    } finally {
      approvalInFlight.current = false;
      setApproving(false);
    }
  }
  return (
    <details className="agent-turn-controls">
      <summary>Agent turn · {status}</summary>
      <div className="agent-turn-actions">
        <Button type="button" variant="plain" size="unstyled" onClick={() => void refresh()}>
          Refresh status & events
        </Button>
        {!terminal ? (
          <Button type="button" variant="plain" size="unstyled" onClick={() => void cancel()}>
            Cancel turn
          </Button>
        ) : null}
        {agent.pendingApproval && status === "awaiting_approval" ? (
          <Button
            type="button"
            variant="plain"
            size="unstyled"
            disabled={approving}
            onClick={() => void resume()}
          >
            {approving ? "Requesting approval…" : "Approve & resume"}
          </Button>
        ) : null}
      </div>
      {notice ? <p role="alert">{notice}</p> : null}
      {events.length ? (
        <ol>
          {events.map((event) => (
            <li key={event.key}>
              <code>{event.text}</code>
            </li>
          ))}
        </ol>
      ) : null}
    </details>
  );
}

export function validateApprovalChallenge(
  challenge: { approvalToken: string; approvalId: string; expiresAtUnixMs: number },
  now: number,
) {
  if (!challenge.approvalToken || !challenge.approvalId)
    throw new Error("Labby returned an incomplete approval challenge.");
  if (challenge.expiresAtUnixMs <= now)
    throw new Error("Labby approval expired before resume; request it again.");
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}
function errorMessage(value: unknown) {
  const record = asRecord(value);
  return typeof record.message === "string"
    ? record.message
    : typeof record.error === "string"
      ? record.error
      : "Axon rejected the agent operation.";
}
