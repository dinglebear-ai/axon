import { Button } from "@/components/ui/aurora/button";
import type { Client } from "@/lib/axonClient";
import {
  type CodexOperation,
  cancelCodexOperation,
  reconcileCodexOperation,
} from "@/lib/codexControl";

interface CodexOperationListProps {
  operations: CodexOperation[];
  client: Client | null;
  busy: boolean;
  run: (action: () => Promise<void>) => Promise<void>;
  refresh: () => Promise<void>;
}

export function CodexOperationList({
  operations,
  client,
  busy,
  run,
  refresh,
}: CodexOperationListProps) {
  if (operations.length === 0) return null;

  async function cancel(operationId: number) {
    await run(async () => {
      if (client) await cancelCodexOperation(client, operationId);
      await refresh();
    });
  }

  async function reconcile(operationId: number, resolveWithoutReplay = false) {
    await run(async () => {
      const note = resolveWithoutReplay
        ? window.prompt("Audit note explaining how you verified the external effect")
        : undefined;
      if (client && (!resolveWithoutReplay || note)) {
        await reconcileCodexOperation(client, operationId, resolveWithoutReplay, true, note ?? undefined);
      }
      await refresh();
    });
  }

  return (
    <section className="codex-mutation">
      <h3>Unfinished operations</h3>
      {operations.map((item) => (
        <article key={item.id}>
          <strong>
            #{item.id} {item.phase}
          </strong>
          <code>{item.request_digest}</code>
          <p>{item.method}</p>
          <p>
            Actor: {item.actor} · Scope: {item.scope}
          </p>
          {item.approver && <p>Approver: {item.approver}</p>}
          {item.recovery_state && <p>Recovery: {item.recovery_state}</p>}
          <pre>{JSON.stringify(item.redacted_request, null, 2)}</pre>
          {["pending", "approved"].includes(item.phase) && (
            <Button disabled={busy || !client} onClick={() => void cancel(item.id)}>
              Cancel operation
            </Button>
          )}
          {["ambiguous", "recovery_required", "rollback_required"].includes(item.phase) && (
            <Button disabled={busy || !client} onClick={() => void reconcile(item.id)}>
              Verify recovery
            </Button>
          )}
          {item.phase === "recovery_required" &&
            item.recovery_state?.includes("explicit_non_replay_disposition_required") && (
              <Button disabled={busy || !client} onClick={() => void reconcile(item.id, true)}>
                Resolve without replay
              </Button>
            )}
        </article>
      ))}
    </section>
  );
}
