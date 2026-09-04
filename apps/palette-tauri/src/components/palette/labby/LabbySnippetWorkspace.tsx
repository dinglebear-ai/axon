import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/aurora/button";
import type { BackendProfile } from "@/lib/backendProfiles/model";
import {
  type LabbyCatalogEntry,
  LabbyClient,
  type LabbySnippetInfo,
  type LabbyToolDescriptor,
} from "@/lib/clients/labbyClient";
import { inertResultPreview } from "@/lib/labby/schemaForm";
import {
  EMPTY_SNIPPET_DRAFT,
  hasUnsavedSnippetChanges,
  insertToolCall,
  parseSnippetParams,
  type SnippetDraft,
} from "@/lib/labby/snippetDraft";

type Validation = { state: "idle" | "checking" | "valid" | "invalid"; message: string };

export function LabbySnippetWorkspace({ profile }: { profile: BackendProfile }) {
  const client = useMemo(() => new LabbyClient(profile), [profile]);
  const [snippets, setSnippets] = useState<LabbySnippetInfo[]>([]);
  const [draft, setDraft] = useState<SnippetDraft>({ ...EMPTY_SNIPPET_DRAFT });
  const [tools, setTools] = useState<LabbyCatalogEntry[]>([]);
  const [toolQuery, setToolQuery] = useState("");
  const [selectedTool, setSelectedTool] = useState<LabbyToolDescriptor | null>(null);
  const [toolArguments, setToolArguments] = useState("{}");
  const [validation, setValidation] = useState<Validation>({ state: "idle", message: "" });
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<string | null>(null);
  const [output, setOutput] = useState<unknown>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [promotionExecutionId, setPromotionExecutionId] = useState("");
  const [confirmPromotion, setConfirmPromotion] = useState(false);
  const generation = useRef(0);
  const activeOperation = useRef<AbortController | null>(null);

  const refresh = useCallback(
    async (signal?: AbortSignal) => {
      const result = await client.listSnippets(signal);
      setSnippets(result.value.snippets);
      setReceipt(`audit ${result.receipt.auditId} · exact/no-LLM`);
    },
    [client],
  );

  useEffect(() => {
    const controller = new AbortController();
    void refresh(controller.signal).catch((reason) => {
      if (!(reason instanceof DOMException && reason.name === "AbortError"))
        setError(String(reason));
    });
    return () => controller.abort();
  }, [refresh]);

  useEffect(() => {
    const controller = new AbortController();
    const current = ++generation.current;
    const timer = window.setTimeout(() => {
      void client
        .search(toolQuery, controller.signal)
        .then((catalog) => {
          if (generation.current === current)
            setTools(catalog.entries.filter((entry) => entry.kind === "mcpTool"));
        })
        .catch((reason) => {
          if (!(reason instanceof DOMException && reason.name === "AbortError"))
            setError(String(reason));
        });
    }, 180);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [client, toolQuery]);

  useEffect(() => {
    if (!draft.name || !draft.body.trim()) {
      setValidation({ state: "idle", message: "Name and code are required." });
      return;
    }
    const controller = new AbortController();
    const current = ++generation.current;
    setValidation({ state: "checking", message: "Validating live with Labby…" });
    const timer = window.setTimeout(() => {
      void client
        .validateSnippet(draft.name, draft.body, controller.signal)
        .then((result) => {
          if (generation.current === current) {
            setValidation({ state: "valid", message: "Labby accepted this draft." });
            setReceipt(`audit ${result.receipt.auditId} · exact/no-LLM validation`);
          }
        })
        .catch((reason) => {
          if (
            generation.current === current &&
            !(reason instanceof DOMException && reason.name === "AbortError")
          )
            setValidation({
              state: "invalid",
              message: reason instanceof Error ? reason.message : String(reason),
            });
        });
    }, 350);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [client, draft.name, draft.body]);

  async function run<T>(
    label: string,
    operation: (signal: AbortSignal) => Promise<T>,
  ): Promise<T | null> {
    activeOperation.current?.abort();
    const controller = new AbortController();
    activeOperation.current = controller;
    setBusy(label);
    setError(null);
    setOutput(null);
    try {
      return await operation(controller.signal);
    } catch (reason) {
      if (reason instanceof DOMException && reason.name === "AbortError")
        setError("Operation cancelled.");
      else setError(reason instanceof Error ? reason.message : String(reason));
      return null;
    } finally {
      if (activeOperation.current === controller) activeOperation.current = null;
      setBusy(null);
    }
  }

  async function openSnippet(item: LabbySnippetInfo) {
    if (hasUnsavedSnippetChanges(draft) && !window.confirm("Discard unsaved snippet edits?"))
      return;
    const result = await run("Loading", (signal) => client.getSnippet(item.name, signal));
    if (!result) return;
    setDraft({
      name: result.value.name,
      description: result.value.description ?? "",
      body: result.value.body,
      baseBody: result.value.body,
      paramsText: "{}",
    });
    setReceipt(`audit ${result.receipt.auditId} · ${result.value.source} source`);
    setConfirmRemove(false);
  }

  async function chooseTool(entry: LabbyCatalogEntry) {
    const result = await run("Loading schema", (signal) => client.descriptor(entry.id, signal));
    if (result) {
      setSelectedTool(result);
      setToolArguments("{}");
    }
  }

  async function save() {
    if (validation.state !== "valid") return;
    const existing = snippets.find((item) => item.name === draft.name);
    const result = await run("Saving", (signal) =>
      client.createSnippet(
        {
          name: draft.name,
          description: draft.description,
          body: draft.body,
          force: Boolean(existing),
        },
        signal,
      ),
    );
    if (!result) return;
    setDraft((value) => ({ ...value, baseBody: value.body }));
    setReceipt(`saved · audit ${result.receipt.auditId} · exact/no-LLM`);
    await refresh();
  }

  async function testOrExecute(kind: "test" | "exec") {
    if (hasUnsavedSnippetChanges(draft)) {
      setError("Save the validated draft before running it.");
      return;
    }
    let params: Record<string, unknown>;
    try {
      params = parseSnippetParams(draft.paramsText);
    } catch (reason) {
      setError(String(reason));
      return;
    }
    const result = await run(kind === "test" ? "Testing" : "Executing", (signal) =>
      kind === "test"
        ? client.testSnippet(draft.name, params, signal)
        : client.executeSnippet(draft.name, params, signal),
    );
    if (!result) return;
    setOutput(result.value);
    setReceipt(
      `${kind} · audit ${result.receipt.auditId} · exact/no-LLM · ${result.receipt.truncated ? "bounded" : "complete"}`,
    );
  }

  async function remove() {
    if (!confirmRemove) return;
    const result = await run("Removing", (signal) => client.removeSnippet(draft.name, signal));
    if (!result) return;
    setDraft({ ...EMPTY_SNIPPET_DRAFT });
    setConfirmRemove(false);
    setOutput(result.value);
    setReceipt(`removed · audit ${result.receipt.auditId}`);
    await refresh();
  }

  async function promote() {
    if (!confirmPromotion || !promotionExecutionId || !draft.name) return;
    const result = await run("Promoting", (signal) =>
      client.promoteSnippet(
        {
          execution_id: promotionExecutionId,
          name: draft.name,
          description: draft.description,
          force: snippets.some((item) => item.name === draft.name && item.source === "user"),
          shadow_builtin: snippets.some(
            (item) => item.name === draft.name && item.source === "builtin",
          ),
        },
        signal,
      ),
    );
    if (!result) return;
    setOutput(result.value);
    setReceipt(`promoted · audit ${result.receipt.auditId} · exact/no-LLM`);
    setConfirmPromotion(false);
    await refresh();
  }

  return (
    <section className="labby-snippets" aria-label="Labby snippet authoring workspace">
      <header className="labby-runner-header">
        <div>
          <strong>Labby snippets</strong>
          <span>{profile.label} · live catalog · lab:admin</span>
        </div>
        {busy && (
          <Button variant="neutral" onClick={() => activeOperation.current?.abort()}>
            Cancel {busy.toLowerCase()}
          </Button>
        )}
      </header>
      <div className="labby-snippet-grid">
        <aside className="labby-snippet-library">
          <Button onClick={() => setDraft({ ...EMPTY_SNIPPET_DRAFT })}>New draft</Button>
          <h2>Library</h2>
          {snippets.map((item) => (
            <Button
              key={`${item.source}:${item.name}`}
              variant="plain"
              size="unstyled"
              onClick={() => void openSnippet(item)}
            >
              <strong>{item.name}</strong>
              <span>
                {item.source} · {item.description ?? "No description"}
              </span>
            </Button>
          ))}
        </aside>
        <main className="labby-snippet-editor">
          <div className="labby-wizard-steps">
            <span>1 Metadata</span>
            <span>2 Live tools</span>
            <span>3 Code</span>
            <span>4 Validate & test</span>
          </div>
          <label>
            Name
            <input
              aria-label="Snippet name"
              value={draft.name}
              disabled={draft.baseBody !== null}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
            />
          </label>
          <label>
            Description
            <input
              aria-label="Snippet description"
              value={draft.description}
              onChange={(e) => setDraft({ ...draft, description: e.target.value })}
            />
          </label>
          <fieldset>
            <legend>Insert a live upstream tool</legend>
            <input
              aria-label="Search tools for snippet"
              value={toolQuery}
              onChange={(e) => setToolQuery(e.target.value)}
              placeholder="Search actual exposed tools…"
            />
            <div className="labby-tool-picker">
              {tools.slice(0, 20).map((tool) => (
                <Button
                  key={`${tool.id}:${tool.contractHash}`}
                  variant="plain"
                  size="unstyled"
                  onClick={() => void chooseTool(tool)}
                >
                  {tool.label}
                  <span>{tool.source}</span>
                </Button>
              ))}
            </div>
            {selectedTool && (
              <div className="labby-tool-insert">
                <code>
                  {selectedTool.id} @ {selectedTool.catalogRevision}
                </code>
                <textarea
                  aria-label="Tool arguments"
                  value={toolArguments}
                  onChange={(e) => setToolArguments(e.target.value)}
                />
                <Button
                  onClick={() => {
                    try {
                      setDraft({
                        ...draft,
                        body: insertToolCall(draft.body, selectedTool, toolArguments),
                      });
                    } catch (reason) {
                      setError(String(reason));
                    }
                  }}
                >
                  Insert exact reference
                </Button>
              </div>
            )}
          </fieldset>
          <label>
            Snippet code
            <textarea
              aria-label="Snippet code"
              value={draft.body}
              onChange={(e) => setDraft({ ...draft, body: e.target.value })}
              spellCheck={false}
            />
          </label>
          <p role="status" className={`labby-validation-${validation.state}`}>
            {validation.message}
          </p>
          <label>
            Test input (JSON object)
            <textarea
              aria-label="Snippet test input"
              value={draft.paramsText}
              onChange={(e) => setDraft({ ...draft, paramsText: e.target.value })}
            />
          </label>
          <div className="labby-snippet-actions">
            <Button
              disabled={validation.state !== "valid" || busy !== null}
              onClick={() => void save()}
            >
              Save revision
            </Button>
            <Button
              variant="neutral"
              disabled={!draft.name || busy !== null}
              onClick={() => void testOrExecute("test")}
            >
              Test exactly
            </Button>
            <Button
              variant="neutral"
              disabled={!draft.name || busy !== null}
              onClick={() => void testOrExecute("exec")}
            >
              Execute
            </Button>
          </div>
          <fieldset>
            <legend>Promote retained live execution</legend>
            <label>
              Execution ID
              <input
                aria-label="Promotion execution ID"
                value={promotionExecutionId}
                onChange={(event) => setPromotionExecutionId(event.target.value)}
              />
            </label>
            <label>
              <input
                type="checkbox"
                checked={confirmPromotion}
                onChange={(event) => setConfirmPromotion(event.target.checked)}
              />{" "}
              Confirm this exact retained execution, target name, and current action contract
            </label>
            <Button
              variant="neutral"
              disabled={!promotionExecutionId || !draft.name || !confirmPromotion || busy !== null}
              onClick={() => void promote()}
            >
              Promote
            </Button>
          </fieldset>
          {draft.baseBody !== null && (
            <div className="labby-remove">
              <label>
                <input
                  type="checkbox"
                  checked={confirmRemove}
                  onChange={(e) => setConfirmRemove(e.target.checked)}
                />{" "}
                Confirm removal of this user snippet
              </label>
              <Button
                variant="destructive"
                disabled={!confirmRemove || busy !== null}
                onClick={() => void remove()}
              >
                Remove
              </Button>
            </div>
          )}
          {error && (
            <p role="alert" className="labby-error">
              {error}
            </p>
          )}
          {receipt && <p className="labby-receipt">{receipt}</p>}
          {output !== null && (
            <pre className="labby-inert-output">{inertResultPreview(output)}</pre>
          )}
        </main>
      </div>
    </section>
  );
}
