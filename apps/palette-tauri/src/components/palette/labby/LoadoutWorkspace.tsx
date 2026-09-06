import { useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/aurora/button";
import {
  assertCompatibleIdentity,
  type BackendProfile,
  type ProductIdentity,
} from "@/lib/backendProfiles/model";
import {
  type CapabilityFamily,
  type CapabilityRef,
  type ExecutionLoadoutPreview,
  type ExecutionLoadoutSummary,
  type LabbyCatalogEntry,
  LabbyClient,
} from "@/lib/clients/labbyClient";
import {
  bufferFrom,
  CAPABILITY_FAMILIES,
  capabilityKey,
  catalogCapability,
  changedFields,
  type LoadoutBuffer,
  MAX_LOADOUT_MEMBERS,
  reapplyBuffer,
  VIRTUALIZE_CATALOG_ABOVE,
  validateBuffer,
} from "@/lib/labby/loadouts/model";

const LOADOUT_CAPABILITY = "loadouts";
const PAGE_SIZE = 100;

export function LoadoutWorkspace({ profile }: { profile: BackendProfile }) {
  const client = useMemo(() => new LabbyClient(profile), [profile]);
  const [identity, setIdentity] = useState<ProductIdentity | null>(null);
  const [items, setItems] = useState<ExecutionLoadoutSummary[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [buffer, setBuffer] = useState<LoadoutBuffer | null>(null);
  const [catalog, setCatalog] = useState<LabbyCatalogEntry[]>([]);
  const [catalogGeneration, setCatalogGeneration] = useState("");
  const [query, setQuery] = useState("");
  const [family, setFamily] = useState<CapabilityFamily>("tool");
  const [manual, setManual] = useState({ provider: "", memberId: "", expectedRevision: "" });
  const [runtimeIdentity, setRuntimeIdentity] = useState("palette");
  const [preview, setPreview] = useState<ExecutionLoadoutPreview | null>(null);
  const [visible, setVisible] = useState(PAGE_SIZE);
  const [rollbackRevision, setRollbackRevision] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const request = useRef<AbortController | null>(null);
  const enabled = identity?.capabilities.includes(LOADOUT_CAPABILITY) ?? false;

  useEffect(() => {
    const abort = new AbortController();
    setBuffer(null);
    setSelected(null);
    setPreview(null);
    setItems([]);
    setCatalog([]);
    client
      .identity(abort.signal)
      .then((r) => setIdentity(assertCompatibleIdentity(profile, r.payload)))
      .catch((e) => {
        if ((e as Error).name !== "AbortError") setNotice(String(e));
      });
    return () => abort.abort();
  }, [client, profile]);
  useEffect(() => () => request.current?.abort(), []);
  useEffect(() => {
    if (!enabled) return;
    const abort = new AbortController();
    client
      .listExecutionLoadouts(abort.signal)
      .then((page) => setItems(page.items.slice(0, 256)))
      .catch((e) => {
        if ((e as Error).name !== "AbortError") setNotice(String(e));
      });
    return () => abort.abort();
  }, [client, enabled]);
  useEffect(() => {
    if (!enabled) return;
    const abort = new AbortController();
    const timer = window.setTimeout(() => {
      client
        .search(query, abort.signal)
        .then((page) => {
          setCatalog(page.entries);
          setCatalogGeneration(page.fingerprint);
          setVisible(PAGE_SIZE);
        })
        .catch((e) => {
          if ((e as Error).name !== "AbortError") setNotice(String(e));
        });
    }, 150);
    return () => {
      window.clearTimeout(timer);
      abort.abort();
    };
  }, [client, enabled, query]);

  async function run(work: (signal: AbortSignal) => Promise<void>) {
    request.current?.abort();
    const abort = new AbortController();
    request.current = abort;
    setBusy(true);
    setNotice(null);
    try {
      await work(abort.signal);
    } catch (e) {
      if ((e as Error).name !== "AbortError") setNotice(String(e));
    } finally {
      if (request.current === abort) setBusy(false);
    }
  }
  async function refreshLibrary(signal?: AbortSignal) {
    const page = await client.listExecutionLoadouts(signal);
    setItems(page.items.slice(0, 256));
  }
  function open(id: string) {
    void run(async (signal) => {
      const value = await client.getExecutionLoadout(id, signal);
      setSelected(id);
      setBuffer(bufferFrom(profile.id, value));
      setPreview(null);
    });
  }
  function update(change: Partial<Pick<LoadoutBuffer, "name" | "description" | "members">>) {
    setBuffer((old) => (old ? { ...old, ...change } : old));
    setPreview(null);
  }
  function toggle(member: CapabilityRef) {
    if (!buffer) return;
    const key = capabilityKey(member);
    const exists = buffer.members.some((item) => capabilityKey(item) === key);
    update({
      members: exists
        ? buffer.members.filter((item) => capabilityKey(item) !== key)
        : [...buffer.members, member].slice(0, MAX_LOADOUT_MEMBERS),
    });
  }
  async function save() {
    if (!buffer) return;
    const invalid = validateBuffer(buffer);
    if (invalid) {
      setNotice(invalid);
      return;
    }
    await run(async (signal) => {
      try {
        const value = await client.patchExecutionLoadout(
          buffer.base.id,
          {
            expectedDraftRevision: buffer.base.draftRevision,
            name: buffer.name,
            description: buffer.description || null,
            members: buffer.members,
          },
          signal,
        );
        setBuffer(bufferFrom(profile.id, value));
        await refreshLibrary(signal);
        setNotice("Draft saved with revision compare-and-swap.");
      } catch (error) {
        if (!isRevisionConflict(error)) throw error;
        const current = await client.getExecutionLoadout(buffer.base.id, signal);
        const fields = changedFields(buffer, current);
        setBuffer(reapplyBuffer(buffer, current));
        setNotice(
          `Draft changed on Labby. Reloaded revision ${current.draftRevision} and reapplied local ${fields.join(", ") || "edits"}; review before saving again.`,
        );
      }
    });
  }
  const liveMembers = catalog
    .map(catalogCapability)
    .filter((item): item is CapabilityRef => item !== null);
  if (!identity)
    return (
      <section className="loadout-workspace">
        <p>Verifying Labby identity and capabilities…</p>
        {notice && <p role="alert">{notice}</p>}
      </section>
    );
  if (!enabled)
    return (
      <section className="loadout-workspace">
        <h2>Execution loadouts unavailable</h2>
        <p>
          This authenticated Labby profile does not advertise <code>{LOADOUT_CAPABILITY}</code>.
          Editing is disabled.
        </p>
      </section>
    );
  return (
    <section className="loadout-workspace">
      <header>
        <div>
          <p className="eyebrow">Labby · {identity.server_id}</p>
          <h2>ExecutionLoadouts</h2>
          <p>Per-turn capability selection. GatewayLoadout routes and restart debt are separate.</p>
        </div>
        <Button
          onClick={() =>
            void run(async (signal) => {
              await refreshLibrary(signal);
            })
          }
          disabled={busy}
        >
          Refresh
        </Button>
      </header>
      <div className="loadout-columns">
        <aside>
          <h3>Library</h3>
          <Button
            onClick={() => {
              const id = `loadout-${Date.now()}`;
              void run(async (signal) => {
                const value = await client.createExecutionLoadout(
                  { id, name: "New loadout", members: [] },
                  signal,
                );
                await refreshLibrary(signal);
                setSelected(value.id);
                setBuffer(bufferFrom(profile.id, value));
              });
            }}
          >
            New draft
          </Button>
          {items.map((item) => (
            <button
              type="button"
              className="loadout-library-row"
              aria-current={selected === item.id}
              key={`${item.id}:${item.draftRevision}`}
              onClick={() => open(item.id)}
            >
              <strong>{item.name}</strong>
              <span>
                draft {item.draftRevision} · desired {item.desiredActiveRevision ?? "none"} ·
                effective {item.effectiveRuntimeRevision ?? "none"}
              </span>
            </button>
          ))}
        </aside>
        <main>
          {!buffer ? (
            <p>Select or create a loadout.</p>
          ) : (
            <>
              <div className="loadout-fields">
                <label>
                  Name
                  <input
                    value={buffer.name}
                    maxLength={256}
                    onChange={(e) => update({ name: e.target.value })}
                  />
                </label>
                <label>
                  Description
                  <input
                    value={buffer.description}
                    maxLength={256}
                    onChange={(e) => update({ description: e.target.value })}
                  />
                </label>
                <label>
                  Runtime identity
                  <input
                    value={runtimeIdentity}
                    maxLength={256}
                    onChange={(e) => setRuntimeIdentity(e.target.value)}
                  />
                </label>
              </div>
              <div className="loadout-actions">
                <Button onClick={() => void save()} disabled={busy}>
                  Save draft
                </Button>
                <Button
                  variant="plain"
                  onClick={() =>
                    void run(async (signal) =>
                      setPreview(
                        await client.previewExecutionLoadout(
                          buffer.base.id,
                          runtimeIdentity,
                          signal,
                        ),
                      ),
                    )
                  }
                  disabled={busy}
                >
                  Preview
                </Button>
                <Button
                  variant="plain"
                  onClick={() =>
                    void run(async (signal) => {
                      const result = await client.activateExecutionLoadout(
                        buffer.base.id,
                        buffer.base.draftRevision,
                        runtimeIdentity,
                        signal,
                      );
                      setBuffer(bufferFrom(profile.id, result.loadout));
                      setPreview(result.preview);
                      await refreshLibrary(signal);
                    })
                  }
                  disabled={busy}
                >
                  Activate
                </Button>
                <input
                  aria-label="Rollback revision"
                  inputMode="numeric"
                  value={rollbackRevision}
                  onChange={(e) => setRollbackRevision(e.target.value)}
                />
                <Button
                  variant="plain"
                  onClick={() =>
                    void run(async (signal) => {
                      const value = await client.rollbackExecutionLoadout(
                        buffer.base.id,
                        buffer.base.draftRevision,
                        Number(rollbackRevision),
                        signal,
                      );
                      setBuffer(bufferFrom(profile.id, value));
                    })
                  }
                  disabled={busy || !/^\d+$/.test(rollbackRevision)}
                >
                  Rollback to draft
                </Button>
              </div>
              <h3>
                Capabilities{" "}
                <span>
                  {buffer.members.length}/{MAX_LOADOUT_MEMBERS}
                </span>
              </h3>
              <div className="loadout-family-tabs">
                {CAPABILITY_FAMILIES.map((value) => (
                  <button
                    type="button"
                    key={value}
                    aria-current={family === value}
                    onClick={() => setFamily(value)}
                  >
                    {value.replace("_", " ")}
                  </button>
                ))}
              </div>
              {family === "tool" ? (
                <>
                  <label>
                    Search live authorized catalog
                    <input value={query} onChange={(e) => setQuery(e.target.value)} />
                  </label>
                  <p className="loadout-generation">
                    Catalog generation {catalogGeneration || "pending"}
                  </p>
                  <div className="loadout-catalog">
                    {liveMembers.slice(0, visible).map((member) => (
                      <label key={`${catalogGeneration}:${capabilityKey(member)}`}>
                        <input
                          type="checkbox"
                          checked={buffer.members.some(
                            (item) => capabilityKey(item) === capabilityKey(member),
                          )}
                          onChange={() => toggle(member)}
                        />
                        {member.memberId}
                        <small>
                          {member.provider} · {member.expectedRevision}
                        </small>
                      </label>
                    ))}
                  </div>
                  {liveMembers.length > visible && (
                    <Button
                      variant="plain"
                      onClick={() => setVisible((n) => Math.min(n + PAGE_SIZE, liveMembers.length))}
                    >
                      Load older
                    </Button>
                  )}
                  {liveMembers.length > VIRTUALIZE_CATALOG_ABOVE && (
                    <p>Showing a bounded {visible}-row window of this catalog generation.</p>
                  )}
                </>
              ) : (
                <div className="loadout-manual">
                  <p>
                    Labby has no authoritative live {family.replace("_", " ")} catalog identity in
                    this API generation. Add an opaque reference only; preview will report
                    unsupported, missing, stale, or unauthorized status explicitly.
                  </p>
                  <input
                    aria-label="Provider identity"
                    placeholder="Provider identity"
                    value={manual.provider}
                    onChange={(e) => setManual({ ...manual, provider: e.target.value })}
                  />
                  <input
                    aria-label="Opaque member ID"
                    placeholder="Opaque member ID"
                    value={manual.memberId}
                    onChange={(e) => setManual({ ...manual, memberId: e.target.value })}
                  />
                  <input
                    aria-label="Expected revision"
                    placeholder="Expected revision"
                    value={manual.expectedRevision}
                    onChange={(e) => setManual({ ...manual, expectedRevision: e.target.value })}
                  />
                  <Button
                    variant="plain"
                    onClick={() => {
                      toggle({ ...manual, family });
                      setManual({ provider: "", memberId: "", expectedRevision: "" });
                    }}
                  >
                    Add reference
                  </Button>
                </div>
              )}
              <ul className="loadout-members">
                {buffer.members
                  .filter((m) => m.family === family)
                  .map((member) => (
                    <li key={capabilityKey(member)}>
                      <code>{member.memberId}</code>
                      <span>
                        {member.provider} · {member.expectedRevision}
                      </span>
                      <Button variant="plain" onClick={() => toggle(member)}>
                        Remove
                      </Button>
                    </li>
                  ))}
              </ul>
              {preview && (
                <section className="loadout-preview">
                  <h3>Server preview</h3>
                  <p>
                    {preview.effective.length} effective · {preview.missing.length} missing ·
                    generation {preview.catalogGeneration}
                  </p>
                  {preview.conflicts.map((value) => (
                    <p role="alert" key={value}>
                      {value}
                    </p>
                  ))}
                  <ul>
                    {preview.resolved.map((item) => (
                      <li key={capabilityKey(item.capability)}>
                        <strong>{item.status}</strong> {item.capability.memberId}
                        {item.diagnostic ? ` — ${item.diagnostic}` : ""}
                      </li>
                    ))}
                  </ul>
                </section>
              )}
            </>
          )}
        </main>
      </div>
      {notice && (
        <p className="loadout-notice" role="status">
          {notice}
        </p>
      )}
    </section>
  );
}

function isRevisionConflict(error: unknown): boolean {
  const detail = (error as { detail?: unknown })?.detail;
  return JSON.stringify(detail ?? "").includes("stale_revision");
}
