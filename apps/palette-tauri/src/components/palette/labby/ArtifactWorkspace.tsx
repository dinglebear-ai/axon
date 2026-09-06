import { useEffect, useMemo, useRef, useState } from "react";
import {
  assertCompatibleIdentity,
  type BackendProfile,
  type ProductIdentity,
} from "@/lib/backendProfiles/model";
import {
  type ArtifactDetail,
  type ArtifactFamily,
  type ArtifactFile,
  type ArtifactHistoryPage,
  type ArtifactPage,
  type ArtifactPreview,
  type ArtifactSummary,
  type ArtifactValidation,
  LabbyClient,
} from "@/lib/clients/labbyClient";
import {
  type ArtifactBuffer,
  boundedText,
  bufferFrom,
  hasUnsavedChanges,
  idempotencyKey,
  LIST_PAGE_SIZE,
  reapply,
  validateBuffer,
} from "@/lib/labby/artifacts/model";
import { ArtifactWorkspaceView } from "./ArtifactWorkspaceView";

type Mode = "form" | "raw";

export function ArtifactWorkspace({ profile }: { profile: BackendProfile }) {
  const client = useMemo(() => new LabbyClient(profile), [profile]);
  const [identity, setIdentity] = useState<ProductIdentity | null>(null);
  const [family, setFamily] = useState<ArtifactFamily>("skill");
  const [page, setPage] = useState<ArtifactPage | null>(null);
  const [items, setItems] = useState<ArtifactSummary[]>([]);
  const [buffer, setBuffer] = useState<ArtifactBuffer | null>(null);
  const [history, setHistory] = useState<ArtifactHistoryPage | null>(null);
  const [preview, setPreview] = useState<ArtifactPreview | null>(null);
  const [validation, setValidation] = useState<ArtifactValidation | null>(null);
  const [diff, setDiff] = useState<unknown>(null);
  const [mode, setMode] = useState<Mode>("form");
  const [fileIndex, setFileIndex] = useState(0);
  const [visible, setVisible] = useState(LIST_PAGE_SIZE);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const request = useRef<AbortController | null>(null);
  const previousProfile = useRef(profile.id);
  const bufferRef = useRef(buffer);
  bufferRef.current = buffer;

  useEffect(() => {
    if (previousProfile.current !== profile.id && hasUnsavedChanges(bufferRef.current)) {
      window.confirm(
        "Profile changed. Unsaved artifact edits cannot cross profiles and will be discarded.",
      );
    }
    previousProfile.current = profile.id;
    request.current?.abort();
    const abort = new AbortController();
    setIdentity(null);
    setPage(null);
    setItems([]);
    setBuffer(null);
    setHistory(null);
    setPreview(null);
    client
      .identity(abort.signal)
      .then((response) => setIdentity(assertCompatibleIdentity(profile, response.payload)))
      .catch((error) => {
        if ((error as Error).name !== "AbortError") setNotice(String(error));
      });
    return () => abort.abort();
  }, [client, profile]);
  useEffect(() => () => request.current?.abort(), []);
  useEffect(() => {
    if (!identity) return;
    const abort = new AbortController();
    setBusy(true);
    client
      .artifactAction<ArtifactPage>(family, "list", { limit: LIST_PAGE_SIZE }, abort.signal)
      .then(({ value }) => {
        setPage(value);
        setItems(value.items);
        setVisible(LIST_PAGE_SIZE);
      })
      .catch((error) => {
        if ((error as Error).name !== "AbortError") setNotice(errorMessage(error));
      })
      .finally(() => setBusy(false));
    return () => abort.abort();
  }, [client, identity, family]);

  async function run<T>(work: (signal: AbortSignal) => Promise<T>): Promise<T | undefined> {
    request.current?.abort();
    const abort = new AbortController();
    request.current = abort;
    setBusy(true);
    setNotice(null);
    try {
      return await work(abort.signal);
    } catch (error) {
      if ((error as Error).name !== "AbortError") setNotice(errorMessage(error));
    } finally {
      if (request.current === abort) {
        request.current = null;
        setBusy(false);
      }
    }
  }
  async function call<T>(
    action: Parameters<LabbyClient["artifactAction"]>[1],
    params: Record<string, unknown>,
    signal?: AbortSignal,
  ) {
    return (await client.artifactAction<T>(family, action, params, signal)).value;
  }
  async function loadList(reset = false) {
    await run(async (signal) => {
      const value = await call<ArtifactPage>(
        "list",
        { limit: LIST_PAGE_SIZE, ...(reset ? {} : { cursor: page?.next_cursor }) },
        signal,
      );
      setPage(value);
      setItems((old) => (reset ? value.items : [...old, ...value.items].slice(0, 500)));
      setVisible(LIST_PAGE_SIZE);
    });
  }
  async function readFiles(detail: ArtifactDetail, signal: AbortSignal): Promise<ArtifactFile[]> {
    return Promise.all(
      detail.latest_revision_files.map(async ({ path }) => {
        const value = await call<{ text: string }>(
          "read",
          { artifact_id: detail.artifact_id, revision_id: detail.latest_revision_id, path },
          signal,
        );
        return { path, content: boundedText(value.text) };
      }),
    );
  }
  function guardDiscard(): boolean {
    return !hasUnsavedChanges(buffer) || window.confirm("Discard unsaved artifact edits?");
  }
  async function open(summary: ArtifactSummary) {
    if (!guardDiscard()) return;
    await run(async (signal) => {
      const detail = await call<ArtifactDetail>(
        "get",
        { artifact_id: summary.artifact_id },
        signal,
      );
      const files = await readFiles(detail, signal);
      setBuffer(bufferFrom(profile.id, family, detail, files));
      setFileIndex(0);
      setPreview(null);
      setValidation(null);
      setDiff(null);
      setHistory(
        await call<ArtifactHistoryPage>(
          "history",
          { artifact_id: detail.artifact_id, limit: LIST_PAGE_SIZE },
          signal,
        ),
      );
    });
  }
  function update(change: Partial<ArtifactBuffer>) {
    setBuffer((old) => (old ? { ...old, ...change } : old));
    setPreview(null);
    setValidation(null);
  }
  function updateFile(change: Partial<ArtifactFile>) {
    if (!buffer) return;
    update({
      files: buffer.files.map((file, index) =>
        index === fileIndex ? { ...file, ...change } : file,
      ),
    });
  }
  async function validateOrPreview(action: "validate" | "preview") {
    if (!buffer) return;
    const invalid = validateBuffer(buffer);
    if (invalid) return setNotice(invalid);
    await run(async (signal) => {
      if (action === "validate") {
        const result = await call<ArtifactValidation>(
          action,
          { name: buffer.name, files: buffer.files },
          signal,
        );
        setValidation(result);
        setNotice(
          result.valid
            ? "Labby validated this inert buffer."
            : "Labby rejected this buffer; review validation details.",
        );
      } else {
        const result = await call<ArtifactPreview>(
          action,
          { name: buffer.name, files: buffer.files },
          signal,
        );
        if (result.render_mode !== "inert_text")
          throw new Error("Labby returned a non-inert preview mode");
        setPreview({
          ...result,
          files: result.files.map((file) => ({ ...file, text: boundedText(file.text) })),
        });
      }
    });
  }
  async function save() {
    if (!buffer || !page) return;
    const invalid = validateBuffer(buffer);
    if (invalid) return setNotice(invalid);
    await run(async (signal) => {
      const action = buffer.artifactId ? "save" : "create";
      const target = buffer.artifactId ?? buffer.name;
      const params: Record<string, unknown> = {
        files: buffer.files,
        expected_library_version: page.library_version,
        idempotency_key: idempotencyKey(action, target),
      };
      if (buffer.artifactId) {
        params.artifact_id = buffer.artifactId;
        params.expected_revision_id = buffer.base?.latest_revision_id;
      } else {
        params.name = buffer.name;
        params.visibility = buffer.visibility;
      }
      try {
        const receipt = await call<{ artifact_id: string }>(action, params, signal);
        await reloadAfterMutation(receipt.artifact_id, signal);
        setNotice("Immutable revision saved; activation remains explicit.");
      } catch (error) {
        if (!isConflict(error) || !buffer.artifactId) throw error;
        const current = await call<ArtifactDetail>(
          "get",
          { artifact_id: buffer.artifactId },
          signal,
        );
        const remoteFiles = await readFiles(current, signal);
        setBuffer(reapply(buffer, current, remoteFiles));
        setDiff(
          await call(
            "diff",
            {
              artifact_id: buffer.artifactId,
              from_revision_id: buffer.base?.latest_revision_id,
              to_revision_id: current.latest_revision_id,
            },
            signal,
          ),
        );
        setNotice(
          `Labby changed since this edit began. Revision ${current.latest_revision_id} is loaded as the CAS base; local text was reapplied. Review the diff, then save again.`,
        );
      }
    });
  }
  async function reloadAfterMutation(id: string, signal: AbortSignal) {
    const nextPage = await call<ArtifactPage>("list", { limit: LIST_PAGE_SIZE }, signal);
    setPage(nextPage);
    setItems(nextPage.items);
    const detail = await call<ArtifactDetail>("get", { artifact_id: id }, signal);
    const files = await readFiles(detail, signal);
    setBuffer(bufferFrom(profile.id, family, detail, files));
    setHistory(await call("history", { artifact_id: id, limit: LIST_PAGE_SIZE }, signal));
  }
  async function lifecycle(
    action: "activate" | "deactivate" | "archive" | "restore" | "rollback",
    revision?: string,
  ) {
    if (!buffer?.artifactId || !buffer.base || !page) return;
    const artifactId = buffer.artifactId;
    const latestRevisionId = buffer.base.latest_revision_id;
    const libraryVersion = page.library_version;
    if (action === "archive" && !window.confirm("Archive this artifact? Revisions remain stored."))
      return;
    await run(async (signal) => {
      const params: Record<string, unknown> = {
        artifact_id: artifactId,
        expected_library_version: libraryVersion,
        idempotency_key: idempotencyKey(action, artifactId),
      };
      if (action === "activate" || action === "rollback")
        params.expected_revision_id = revision ?? latestRevisionId;
      await call(action, params, signal);
      await reloadAfterMutation(artifactId, signal);
      setNotice(`${action} committed by Labby.`);
    });
  }

  async function inspectRevision(revisionId: string) {
    if (!buffer?.artifactId || !buffer.files[fileIndex]) return;
    const artifactId = buffer.artifactId;
    const path = buffer.files[fileIndex].path;
    await run(async (signal) => {
      const file = await call<{ path: string; text: string }>(
        "read",
        {
          artifact_id: artifactId,
          revision_id: revisionId,
          path,
        },
        signal,
      );
      setPreview({
        artifact_id: artifactId,
        revision_id: revisionId,
        render_mode: "inert_text",
        files: [
          {
            path: file.path,
            media_type: "text/plain; charset=utf-8",
            text: boundedText(file.text),
          },
        ],
      });
    });
  }

  async function diffRevision(revisionId: string) {
    if (!buffer?.artifactId || !buffer.base) return;
    const artifactId = buffer.artifactId;
    const latestRevisionId = buffer.base.latest_revision_id;
    await run(async (signal) => {
      setDiff(
        await call(
          "diff",
          {
            artifact_id: artifactId,
            from_revision_id: revisionId,
            to_revision_id: latestRevisionId,
          },
          signal,
        ),
      );
    });
  }

  if (!identity)
    return (
      <section className="artifact-workspace">
        <p>Verifying Labby identity, authorization, and capabilities…</p>
        {notice && <p role="alert">{notice}</p>}
      </section>
    );
  const familyActions =
    page?.allowed_actions.some((action) => action.startsWith(`${family}_library.`)) ?? false;
  return (
    <ArtifactWorkspaceView
      {...{
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
      }}
    />
  );
}

function isConflict(error: unknown): boolean {
  const text = errorMessage(error).toLowerCase();
  return text.includes("stale") || text.includes("conflict") || text.includes("version");
}
function errorMessage(error: unknown): string {
  return error instanceof Error
    ? `${error.message}${"detail" in error ? `: ${JSON.stringify((error as Error & { detail: unknown }).detail)}` : ""}`
    : String(error);
}
