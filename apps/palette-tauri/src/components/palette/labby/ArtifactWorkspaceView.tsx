import type { Dispatch, SetStateAction } from "react";
import { Button } from "@/components/ui/aurora/button";
import type { BackendProfile, ProductIdentity } from "@/lib/backendProfiles/model";
import type {
  ArtifactFamily,
  ArtifactHistoryPage,
  ArtifactPage,
  ArtifactPreview,
  ArtifactSummary,
  ArtifactValidation,
} from "@/lib/clients/labbyClient";
import {
  ARTIFACT_FAMILIES,
  type ArtifactBuffer,
  boundedText,
  emptyBuffer,
  LIST_PAGE_SIZE,
  VIRTUALIZE_ABOVE,
} from "@/lib/labby/artifacts/model";

type Mode = "form" | "raw";

interface ArtifactWorkspaceViewProps {
  profile: BackendProfile;
  identity: ProductIdentity;
  family: ArtifactFamily;
  page: ArtifactPage | null;
  items: ArtifactSummary[];
  buffer: ArtifactBuffer | null;
  history: ArtifactHistoryPage | null;
  preview: ArtifactPreview | null;
  validation: ArtifactValidation | null;
  diff: unknown;
  mode: Mode;
  fileIndex: number;
  visible: number;
  notice: string | null;
  busy: boolean;
  familyActions: boolean;
  guardDiscard: () => boolean;
  open: (item: ArtifactSummary) => Promise<void>;
  loadList: (reset?: boolean) => Promise<void>;
  update: (change: Partial<ArtifactBuffer>) => void;
  updateFile: (change: Partial<ArtifactBuffer["files"][number]>) => void;
  validateOrPreview: (action: "validate" | "preview") => Promise<void>;
  save: () => Promise<void>;
  lifecycle: (
    action: "activate" | "deactivate" | "archive" | "restore" | "rollback",
    revision?: string,
  ) => Promise<void>;
  inspectRevision: (revisionId: string) => Promise<void>;
  diffRevision: (revisionId: string) => Promise<void>;
  setFamily: Dispatch<SetStateAction<ArtifactFamily>>;
  setBuffer: Dispatch<SetStateAction<ArtifactBuffer | null>>;
  setHistory: Dispatch<SetStateAction<ArtifactHistoryPage | null>>;
  setFileIndex: Dispatch<SetStateAction<number>>;
  setVisible: Dispatch<SetStateAction<number>>;
  setMode: Dispatch<SetStateAction<Mode>>;
}

export function ArtifactWorkspaceView(props: ArtifactWorkspaceViewProps) {
  const {
    profile,
    identity,
    family,
    page,
    items,
    buffer,
    history,
    preview,
    validation,
    diff,
    mode,
    fileIndex,
    visible,
    notice,
    busy,
    familyActions,
    guardDiscard,
    open,
    loadList,
    update,
    updateFile,
    validateOrPreview,
    save,
    lifecycle,
    inspectRevision,
    diffRevision,
    setFamily,
    setBuffer,
    setHistory,
    setFileIndex,
    setVisible,
    setMode,
  } = props;
  return (
    <section className="artifact-workspace" aria-label="Labby AI artifact authoring">
      <header>
        <div>
          <p className="eyebrow">Labby · {identity.server_id}</p>
          <h2>AI artifacts</h2>
          <p>
            Labby owns revisions and activation. Palette previews inert text only and never runs
            hooks.
          </p>
        </div>
        <Button onClick={() => void loadList(true)} disabled={busy}>
          Refresh
        </Button>
      </header>
      <nav className="artifact-tabs" aria-label="Artifact families">
        {ARTIFACT_FAMILIES.map((item) => (
          <Button
            key={item}
            variant={family === item ? "aurora" : "plain"}
            onClick={() => {
              if (family === item || !guardDiscard()) return;
              setFamily(item);
              setBuffer(null);
              setHistory(null);
            }}
          >
            {item}s
          </Button>
        ))}
      </nav>
      {!familyActions && page ? (
        <p role="alert">
          This principal is not authorized for {family} artifact actions. Editing is disabled.
        </p>
      ) : (
        <div className="artifact-grid">
          <aside>
            <h3>Library</h3>
            {page?.can_create && (
              <Button
                onClick={() => {
                  if (guardDiscard()) {
                    setBuffer(emptyBuffer(profile.id, family));
                    setHistory(null);
                    setFileIndex(0);
                  }
                }}
              >
                New {family}
              </Button>
            )}
            {items.slice(0, visible).map((item) => (
              <button
                type="button"
                key={item.artifact_id}
                aria-current={buffer?.artifactId === item.artifact_id}
                onClick={() => void open(item)}
              >
                <strong>{item.name}</strong>
                <span>
                  {item.archived ? "archived" : item.active_revision_id ? "active" : "inactive"} ·{" "}
                  {item.access_label}
                </span>
              </button>
            ))}
            {items.length > VIRTUALIZE_ABOVE && visible < items.length && (
              <Button variant="plain" onClick={() => setVisible((n) => n + LIST_PAGE_SIZE)}>
                Render more
              </Button>
            )}
            {page?.next_cursor && (
              <Button variant="plain" onClick={() => void loadList(false)}>
                Load next page
              </Button>
            )}
          </aside>
          <main>
            {!buffer ? (
              <p>Select or create an artifact.</p>
            ) : (
              <>
                <div className="artifact-fields">
                  <label>
                    Name
                    <input
                      value={buffer.name}
                      disabled={!!buffer.artifactId}
                      maxLength={128}
                      onChange={(e) => update({ name: e.target.value })}
                    />
                  </label>
                  <label>
                    Visibility
                    <select
                      value={buffer.visibility}
                      disabled={!!buffer.artifactId}
                      onChange={(e) =>
                        update({ visibility: e.target.value as "private" | "shared" })
                      }
                    >
                      <option value="private">Private</option>
                      <option value="shared">Shared</option>
                    </select>
                  </label>
                  <span>revision {buffer.base?.latest_revision_id ?? "unsaved"}</span>
                </div>
                <div className="artifact-actions">
                  <Button
                    onClick={() => void save()}
                    disabled={busy || buffer.base?.can_mutate === false}
                  >
                    Save revision
                  </Button>
                  <Button
                    variant="plain"
                    onClick={() => void validateOrPreview("validate")}
                    disabled={busy}
                  >
                    Validate
                  </Button>
                  <Button
                    variant="plain"
                    onClick={() => void validateOrPreview("preview")}
                    disabled={busy}
                  >
                    Inert preview
                  </Button>
                  {buffer.base && (
                    <>
                      <Button
                        variant="plain"
                        onClick={() => void lifecycle("activate")}
                        disabled={busy}
                      >
                        Activate
                      </Button>
                      <Button
                        variant="plain"
                        onClick={() => void lifecycle("deactivate")}
                        disabled={busy}
                      >
                        Deactivate
                      </Button>
                      <Button
                        variant="plain"
                        onClick={() =>
                          void lifecycle(buffer.base?.archived ? "restore" : "archive")
                        }
                        disabled={busy}
                      >
                        {buffer.base.archived ? "Restore" : "Archive"}
                      </Button>
                    </>
                  )}
                </div>
                <div className="artifact-mode">
                  <Button
                    variant={mode === "form" ? "aurora" : "plain"}
                    onClick={() => setMode("form")}
                  >
                    Safe form
                  </Button>
                  <Button
                    variant={mode === "raw" ? "aurora" : "plain"}
                    onClick={() => setMode("raw")}
                  >
                    Raw files
                  </Button>
                </div>
                <div className="artifact-file-tabs">
                  {buffer.files.map((file, index) => (
                    <button
                      type="button"
                      key={file.path}
                      aria-current={index === fileIndex}
                      onClick={() => setFileIndex(index)}
                    >
                      {file.path || "unnamed"}
                    </button>
                  ))}
                  <Button
                    variant="plain"
                    onClick={() => {
                      if (buffer.files.length < 64) {
                        update({
                          files: [...buffer.files, { path: "references/notes.md", content: "" }],
                        });
                        setFileIndex(buffer.files.length);
                      }
                    }}
                  >
                    Add file
                  </Button>
                </div>
                {buffer.files[fileIndex] && (
                  <div className="artifact-editor">
                    <label>
                      Logical path
                      <input
                        value={buffer.files[fileIndex].path}
                        disabled={mode === "form" && fileIndex === 0}
                        onChange={(e) => updateFile({ path: e.target.value })}
                      />
                    </label>
                    <label>
                      {mode === "raw" ? "Raw inert text" : `${family} content`}
                      <textarea
                        value={buffer.files[fileIndex].content}
                        spellCheck={false}
                        onChange={(e) => updateFile({ content: e.target.value })}
                      />
                    </label>
                  </div>
                )}
                {validation && (
                  <pre className="artifact-inert" title="Validation result">
                    {JSON.stringify(validation, null, 2)}
                  </pre>
                )}
                {preview && (
                  <section>
                    <h3>Inert preview</h3>
                    {preview.files.map((file) => (
                      <pre className="artifact-inert" key={file.path}>
                        {file.text}
                      </pre>
                    ))}
                  </section>
                )}
                {diff != null && (
                  <pre className="artifact-inert" title="Conflict diff">
                    {boundedText(JSON.stringify(diff, null, 2))}
                  </pre>
                )}
                {history && (
                  <section className="artifact-history">
                    <h3>History</h3>
                    {history.items.slice(0, VIRTUALIZE_ABOVE).map((revision) => (
                      <div key={revision.revision_id}>
                        <code>{revision.revision_id}</code>
                        <Button
                          variant="plain"
                          onClick={() => void inspectRevision(revision.revision_id)}
                          disabled={busy}
                        >
                          Read selected file
                        </Button>
                        <Button
                          variant="plain"
                          onClick={() => void diffRevision(revision.revision_id)}
                          disabled={
                            busy || revision.revision_id === buffer.base?.latest_revision_id
                          }
                        >
                          Diff with latest
                        </Button>
                        <Button
                          variant="plain"
                          onClick={() => void lifecycle("rollback", revision.revision_id)}
                          disabled={busy}
                        >
                          Rollback
                        </Button>
                      </div>
                    ))}
                  </section>
                )}
              </>
            )}
          </main>
        </div>
      )}
      {notice && (
        <p className="artifact-notice" role="status">
          {notice}
        </p>
      )}
    </section>
  );
}
