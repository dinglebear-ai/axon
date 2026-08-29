import type { Dispatch, SetStateAction } from "react";

import { Button } from "@/components/ui/aurora/button";
import type { CodexOperation } from "@/lib/codexControl";
import type { MutationKind } from "./CodexControlView";

type StringSetter = Dispatch<SetStateAction<string>>;

interface CodexMutationEditorProps {
  kind: MutationKind;
  setKind: Dispatch<SetStateAction<MutationKind>>;
  target: string;
  setTarget: StringSetter;
  value: string;
  setValue: StringSetter;
  source: string;
  setSource: StringSetter;
  sha256: string;
  setSha256: StringSetter;
  mcpCommand: string;
  setMcpCommand: StringSetter;
  mcpArgs: string;
  setMcpArgs: StringSetter;
  mcpUrl: string;
  setMcpUrl: StringSetter;
  mcpEnv: string;
  setMcpEnv: StringSetter;
  mcpRemove: boolean;
  setMcpRemove: Dispatch<SetStateAction<boolean>>;
  validationError: string | null;
  busy: boolean;
  canPrepare: boolean;
  operation: CodexOperation | null;
  capability: string;
  onPrepare: () => Promise<void>;
  onApprove: () => Promise<void>;
  onExecute: () => Promise<void>;
}

export function CodexMutationEditor(props: CodexMutationEditorProps) {
  const { kind } = props;
  return (
    <section className="codex-mutation">
      <h3>Approved change</h3>
      <p>Prepare the exact request, approve its digest, then execute the single-use capability.</p>
      <label>
        Workflow
        <select
          value={kind}
          onChange={(event) => props.setKind(event.target.value as MutationKind)}
        >
          <option value="accountLogin">Start account login</option>
          <option value="accountLoginCancel">Cancel account login</option>
          <option value="accountLogout">Log out account</option>
          <option value="config">Write config value</option>
          <option value="mcpConfig">Add, edit, or remove MCP definition</option>
          <option value="configBatch">Write config batch</option>
          <option value="mcpReload">Reload MCP servers</option>
          <option value="mcpOauth">Start MCP OAuth</option>
          <option value="mcpTool">Call MCP tool (approved)</option>
          <option value="mcpStreamStart">Start MCP event stream</option>
          <option value="mcpStreamStop">Stop MCP event stream</option>
          <option value="pluginInstall">Install plugin</option>
          <option value="pluginUninstall">Uninstall plugin</option>
          <option value="pluginShareCheckout">Checkout plugin share</option>
          <option value="pluginShareSave">Save plugin share</option>
          <option value="pluginShareDelete">Delete plugin share</option>
          <option value="pluginShareTargets">Update plugin share targets</option>
          <option value="marketplaceAdd">Add marketplace</option>
          <option value="marketplaceRemove">Remove marketplace</option>
          <option value="marketplaceUpgrade">Upgrade marketplace</option>
          <option value="skillConfig">Enable, disable, or configure skill</option>
          <option value="skillRoots">Set extra skill roots</option>
          <option value="skillImport">Import standalone skill or agent config</option>
          <option value="importHistory">Record external import history</option>
        </select>
      </label>
      {kind !== "configBatch" && (
        <label>
          Target
          <input
            value={props.target}
            onChange={(event) => props.setTarget(event.target.value)}
            placeholder="Config key, MCP server, plugin, marketplace, or skill"
          />
        </label>
      )}
      {kind === "mcpConfig" ? (
        <McpFields {...props} />
      ) : kind === "configBatch" ? (
        <label>
          Batch writes (JSON array or object)
          <textarea
            value={props.value}
            onChange={(event) => props.setValue(event.target.value)}
            placeholder={
              '[{"keyPath":"model","mergeStrategy":"upsert","value":"gpt-5"},{"keyPath":"approval_policy","mergeStrategy":"replace","value":"on-request"}]'
            }
          />
        </label>
      ) : (
        <label>
          {kind === "config" ? "Value (JSON)" : "Value"}
          <input
            value={props.value}
            onChange={(event) => props.setValue(event.target.value)}
            placeholder="Value, enabled state, or OAuth provider"
          />
        </label>
      )}
      {props.validationError && (
        <p className="settings-error" role="alert">
          {props.validationError}
        </p>
      )}
      {(kind === "pluginInstall" || kind === "marketplaceAdd" || kind === "skillImport") && (
        <>
          <label>
            Pinned HTTPS source
            <input
              value={props.source}
              onChange={(event) => props.setSource(event.target.value)}
              placeholder="https://…"
            />
          </label>
          <label>
            SHA-256 digest
            <input
              value={props.sha256}
              onChange={(event) => props.setSha256(event.target.value)}
              maxLength={64}
            />
          </label>
        </>
      )}
      <div className="codex-control-actions">
        <Button disabled={props.busy || !props.canPrepare} onClick={() => void props.onPrepare()}>
          1 Prepare
        </Button>
        <Button disabled={props.busy || !props.operation} onClick={() => void props.onApprove()}>
          2 Approve
        </Button>
        <Button disabled={props.busy || !props.capability} onClick={() => void props.onExecute()}>
          3 Execute
        </Button>
      </div>
      {props.operation && <pre>{JSON.stringify(props.operation, null, 2)}</pre>}
    </section>
  );
}

function McpFields(props: CodexMutationEditorProps) {
  return (
    <>
      <label>
        Command
        <input
          value={props.mcpCommand}
          onChange={(event) => props.setMcpCommand(event.target.value)}
          placeholder="Executable only"
          disabled={props.mcpRemove}
        />
      </label>
      <label>
        Arguments (JSON array)
        <input
          value={props.mcpArgs}
          onChange={(event) => props.setMcpArgs(event.target.value)}
          disabled={props.mcpRemove}
        />
      </label>
      <label>
        HTTPS URL
        <input
          value={props.mcpUrl}
          onChange={(event) => props.setMcpUrl(event.target.value)}
          placeholder="https://…"
          disabled={props.mcpRemove}
        />
      </label>
      <label>
        Environment secret references
        <textarea
          value={props.mcpEnv}
          onChange={(event) => props.setMcpEnv(event.target.value)}
          placeholder="TOKEN=env:MY_TOKEN"
          disabled={props.mcpRemove}
        />
      </label>
      <label>
        <input
          type="checkbox"
          checked={props.mcpRemove}
          onChange={(event) => props.setMcpRemove(event.target.checked)}
        />{" "}
        Remove this MCP definition
      </label>
    </>
  );
}
