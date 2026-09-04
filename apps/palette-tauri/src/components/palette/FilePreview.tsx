import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  Braces,
  Copy,
  FileArchive,
  FileText,
  FolderSearch,
  GitCompare,
  MessageCircleQuestion,
  PanelRight,
  Save,
  Search,
  Sparkles,
  Upload,
  WrapText,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/aurora/button";
import { computeLineDiff } from "@/lib/aiEditModel";
import {
  countTextMatches,
  extractFileSymbols,
  languageForFile,
  selectionPrompt,
} from "@/lib/filesEnhancements";
import { formatBytes, formatModified, isMarkdownLike } from "@/lib/filesModel";
import { AiEditPanel } from "./AiEditPanel";
import { FilesContextMenu } from "./FilesContextMenu";
import { type FilePreviewProps, SYNTAX_LANGUAGES } from "./filePreviewTypes";
import { HighlightedCodeEditor } from "./HighlightedCodeEditor";
import { HighlightedFileCode } from "./HighlightedFileCode";
import { MarkdownBodyInner } from "./MarkdownBodyInner";

export function FilePreview({
  selectedPath,
  absolutePath,
  modifiedUnix,
  file,
  editing,
  draft,
  saving,
  indexState,
  canIndex,
  canEdit,
  onEdit,
  onCancelEdit,
  onDraftChange,
  onSave,
  onIndex,
  sparkleOpen,
  sparkleQuery,
  proposal,
  proposalState,
  proposalErrorMessage,
  onSparkleToggle,
  onSparkleQueryChange,
  onSparkleSubmit,
  onProposalDeny,
  onProposalApprove,
}: FilePreviewProps) {
  const name = selectedPath.split("/").pop() ?? selectedPath;
  const [wrapLines, setWrapLines] = useState(true);
  const [markdownPreview, setMarkdownPreview] = useState(true);
  const [copied, setCopied] = useState(false);
  const [findOpen, setFindOpen] = useState(false);
  const [findQuery, setFindQuery] = useState("");
  const [symbolsOpen, setSymbolsOpen] = useState(false);
  const [minimapEnabled, setMinimapEnabled] = useState(
    () => localStorage.getItem("axon.files.minimap") !== "false",
  );
  const [language, setLanguage] = useState(() => languageForFile(name));
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const findInputRef = useRef<HTMLInputElement>(null);
  const lineCount = file.content.length === 0 ? 0 : file.content.split("\n").length;
  const previewTruncated = file.content.length > 750_000;
  const previewContent = previewTruncated ? file.content.slice(0, 750_000) : file.content;
  const previewLineCount = previewContent.length === 0 ? 0 : previewContent.split("\n").length;
  const symbols = useMemo(() => extractFileSymbols(file.content), [file.content]);
  const findMatches = useMemo(
    () => countTextMatches(editing ? draft : file.content, findQuery),
    [draft, editing, file.content, findQuery],
  );
  const manualDiff = useMemo(
    () => (editing ? computeLineDiff(file.content, draft) : []),
    [draft, editing, file.content],
  );
  const changedLines = manualDiff.filter((line) => line.kind !== "same").length;

  useEffect(() => {
    setLanguage(languageForFile(name));
    setFindQuery("");
    setFindOpen(false);
    setSymbolsOpen(false);
  }, [name]);
  useEffect(
    () => localStorage.setItem("axon.files.minimap", String(minimapEnabled)),
    [minimapEnabled],
  );
  useEffect(() => {
    if (findOpen) findInputRef.current?.focus();
  }, [findOpen]);

  function copyPath() {
    void navigator.clipboard.writeText(selectedPath).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    });
  }

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: The preview surface exposes a contextual right-click menu.
    <div
      className="files-preview-inner"
      onContextMenu={(event) => {
        event.preventDefault();
        setContextMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      <div className="files-preview-header">
        <div className="files-preview-identity" title={selectedPath}>
          <span className="files-preview-file-icon">
            <FileText size={15} />
          </span>
          <span>
            <strong className="files-preview-name">{name}</strong>
            <small className="files-preview-meta">
              {formatBytes(file.size)} · modified {formatModified(modifiedUnix)}
            </small>
          </span>
        </div>
        <div className="files-preview-actions">
          {editing ? (
            <>
              <Button variant="ghost" size="sm" type="button" onClick={onCancelEdit}>
                Cancel
              </Button>
              <Button variant="aurora" size="sm" type="button" onClick={onSave} disabled={saving}>
                <Save size={13} />
                {saving ? "Saving..." : "Save"}
              </Button>
            </>
          ) : (
            <>
              <Button
                variant="plain"
                size="unstyled"
                type="button"
                title={copied ? "Copied" : "Copy path"}
                aria-label={copied ? "Path copied" : "Copy path"}
                onClick={copyPath}
              >
                <Copy size={14} />
              </Button>
              {absolutePath && (
                <Button
                  variant="plain"
                  size="unstyled"
                  type="button"
                  title="Show in Finder"
                  aria-label="Show in Finder"
                  onClick={() => void revealItemInDir(absolutePath)}
                >
                  <FolderSearch size={14} />
                </Button>
              )}
              {canEdit && (
                <Button variant="ghost" size="sm" type="button" onClick={onEdit}>
                  Edit
                </Button>
              )}
              {canEdit && (
                <Button
                  variant="plain"
                  size="unstyled"
                  type="button"
                  title="Edit with the model"
                  aria-label="Edit with the model"
                  onClick={onSparkleToggle}
                >
                  <Sparkles size={14} />
                </Button>
              )}
              {canIndex && (
                <Button
                  variant="aurora"
                  size="sm"
                  type="button"
                  onClick={onIndex}
                  disabled={indexState.kind === "running"}
                >
                  <Upload size={13} />
                  {indexState.kind === "running" ? "Indexing..." : "Index"}
                </Button>
              )}
            </>
          )}
        </div>
      </div>
      {indexState.kind === "done" && (
        <div className={`files-index-status${indexState.ok ? "" : " files-index-status-error"}`}>
          {indexState.message}
        </div>
      )}
      {editing ? (
        <div className="files-edit-workspace">
          <div className="files-edit-status">
            <span className={changedLines ? "is-dirty" : ""}>
              <GitCompare size={12} />
              {changedLines ? `${changedLines} changed lines` : "No changes"}
            </span>
            <span>Tab inserts two spaces</span>
          </div>
          <HighlightedCodeEditor value={draft} language={language} onChange={onDraftChange} />
        </div>
      ) : file.dataUrl && file.mediaType ? (
        <div className="files-media-stage">
          {file.mediaType.startsWith("image/") ? (
            <img src={file.dataUrl} alt={`Preview of ${name}`} />
          ) : file.mediaType === "application/pdf" ? (
            <iframe src={file.dataUrl} title={`Preview of ${name}`} />
          ) : file.mediaType.startsWith("audio/") ? (
            // biome-ignore lint/a11y/useMediaCaption: Local audio previews do not have a separate caption track.
            <audio src={file.dataUrl} controls aria-label={`Preview of ${name}`} />
          ) : file.mediaType.startsWith("video/") ? (
            // biome-ignore lint/a11y/useMediaCaption: Local video previews do not have a separate caption track.
            <video src={file.dataUrl} controls aria-label={`Preview of ${name}`} />
          ) : null}
        </div>
      ) : file.content.length === 0 && file.size > 0 ? (
        <div className="files-binary-stage">
          <FileArchive size={28} />
          <strong>Preview unavailable</strong>
          <p>
            This {formatBytes(file.size)} file does not expose readable text or supported media.
          </p>
          <div>
            <span>{languageForFile(name)}</span>
            <span>binary or packaged content</span>
          </div>
          {absolutePath ? (
            <Button
              variant="ghost"
              size="sm"
              type="button"
              onClick={() => void revealItemInDir(absolutePath)}
            >
              Show in Finder
            </Button>
          ) : null}
        </div>
      ) : (
        <div className="files-document">
          <div className="files-document-toolbar">
            <span>
              {lineCount} {lineCount === 1 ? "line" : "lines"}
            </span>
            <span>{formatBytes(file.size)}</span>
            <label className="files-language-picker">
              <span className="sr-only">Syntax language</span>
              <Braces size={12} />
              <select value={language} onChange={(event) => setLanguage(event.target.value)}>
                {[language, ...SYNTAX_LANGUAGES]
                  .filter((value, index, values) => values.indexOf(value) === index)
                  .map((value) => (
                    <option key={value} value={value}>
                      {value}
                    </option>
                  ))}
              </select>
            </label>
            <button
              type="button"
              className={findOpen ? "is-active" : ""}
              aria-pressed={findOpen}
              onClick={() => setFindOpen((value) => !value)}
            >
              <Search size={13} /> Find
            </button>
            {symbols.length > 0 && (
              <button
                type="button"
                className={symbolsOpen ? "is-active" : ""}
                aria-pressed={symbolsOpen}
                onClick={() => setSymbolsOpen((value) => !value)}
              >
                <Braces size={13} /> {symbols.length} symbols
              </button>
            )}
            {lineCount > 200 && (
              <button
                type="button"
                className={minimapEnabled ? "is-active" : ""}
                aria-pressed={minimapEnabled}
                onClick={() => setMinimapEnabled((value) => !value)}
              >
                <PanelRight size={13} /> Minimap
              </button>
            )}
            {isMarkdownLike(name) && (
              <button
                type="button"
                className={markdownPreview ? "is-active" : ""}
                aria-pressed={markdownPreview}
                onClick={() => setMarkdownPreview((value) => !value)}
              >
                {markdownPreview ? "Preview" : "Raw"}
              </button>
            )}
            <button
              type="button"
              className={wrapLines ? "is-active" : ""}
              aria-pressed={wrapLines}
              onClick={() => setWrapLines((value) => !value)}
            >
              <WrapText size={13} />
              Wrap
            </button>
          </div>
          {findOpen && (
            <div className="files-findbar">
              <Search size={13} />
              <input
                ref={findInputRef}
                type="search"
                value={findQuery}
                onChange={(event) => setFindQuery(event.currentTarget.value)}
                placeholder="Find in file"
                aria-label="Find in file"
              />
              <span>{findQuery ? `${findMatches} matches` : "Type to search"}</span>
              <button
                type="button"
                onClick={() => {
                  setFindOpen(false);
                  setFindQuery("");
                }}
              >
                ×
              </button>
            </div>
          )}
          {previewTruncated ? (
            <div className="files-large-notice" role="status">
              Large file · showing the first {formatBytes(previewContent.length)} of{" "}
              {formatBytes(file.content.length)}. Editing and saving still use the complete file.
            </div>
          ) : null}
          {symbolsOpen && symbols.length > 0 && (
            <nav className="files-symbol-strip" aria-label="File symbols">
              {symbols.map((symbol) => (
                <button
                  type="button"
                  key={`${symbol.line}:${symbol.label}`}
                  title={`Line ${symbol.line}`}
                  onClick={() => {
                    const line = document.querySelector(
                      `.files-shiki .line:nth-child(${symbol.line})`,
                    );
                    line?.scrollIntoView({ block: "center", behavior: "smooth" });
                  }}
                >
                  <span>{symbol.kind}</span>
                  {symbol.label}
                  <em>{symbol.line}</em>
                </button>
              ))}
            </nav>
          )}
          {isMarkdownLike(name) && markdownPreview ? (
            <div className="files-markdown-preview">
              <span className="sr-only" aria-hidden="true">
                {file.content}
              </span>
              <MarkdownBodyInner>{file.content}</MarkdownBodyInner>
            </div>
          ) : (
            <div className={`files-code-surface${wrapLines ? " is-wrapped" : ""}`}>
              <pre className="files-line-numbers" aria-hidden="true">
                {Array.from(
                  { length: Math.max(previewLineCount, 1) },
                  (_, index) => index + 1,
                ).join("\n")}
              </pre>
              <div className="files-preview-text files-preview-code">
                <HighlightedFileCode
                  code={previewContent}
                  language={language}
                  searchQuery={findQuery}
                />
              </div>
              {lineCount > 200 && minimapEnabled ? (
                <pre className="files-minimap" aria-hidden="true">
                  {previewContent}
                </pre>
              ) : null}
            </div>
          )}
        </div>
      )}
      <AiEditPanel
        sparkleOpen={sparkleOpen}
        sparkleQuery={sparkleQuery}
        proposal={proposal}
        proposalState={proposalState}
        proposalErrorMessage={proposalErrorMessage}
        onSparkleToggle={onSparkleToggle}
        onSparkleQueryChange={onSparkleQueryChange}
        onSparkleSubmit={onSparkleSubmit}
        onProposalDeny={onProposalDeny}
        onProposalApprove={onProposalApprove}
      />
      {contextMenu && (
        <FilesContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          label={`${name} actions`}
          items={[
            {
              label: "Copy contents",
              icon: Copy,
              onSelect: () => void navigator.clipboard.writeText(file.content),
            },
            {
              label: "Ask about selection",
              icon: MessageCircleQuestion,
              onSelect: () => {
                const selection = window.getSelection()?.toString().trim() || file.content;
                if (!sparkleOpen) onSparkleToggle();
                onSparkleQueryChange(selectionPrompt("ask", selection));
              },
            },
            {
              label: "Explain selection",
              icon: Sparkles,
              onSelect: () => {
                const selection = window.getSelection()?.toString().trim() || file.content;
                if (!sparkleOpen) onSparkleToggle();
                onSparkleQueryChange(selectionPrompt("explain", selection));
              },
            },
            {
              label: "Copy path",
              icon: FileText,
              onSelect: copyPath,
            },
            ...(absolutePath
              ? [
                  {
                    label: "Show in Finder",
                    icon: FolderSearch,
                    onSelect: () => void revealItemInDir(absolutePath),
                  },
                ]
              : []),
            ...(canEdit
              ? [
                  {
                    label: "Edit file",
                    icon: FileText,
                    separatorBefore: true,
                    onSelect: onEdit,
                  },
                  {
                    label: "Edit with Axon",
                    icon: Sparkles,
                    onSelect: onSparkleToggle,
                  },
                ]
              : []),
            ...(canIndex
              ? [
                  {
                    label: indexState.kind === "running" ? "Indexing…" : "Index file",
                    icon: Upload,
                    separatorBefore: !canEdit,
                    disabled: indexState.kind === "running",
                    onSelect: onIndex,
                  },
                ]
              : []),
          ]}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}
