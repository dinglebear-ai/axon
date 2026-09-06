import { type KeyboardEvent, type UIEvent, useMemo, useRef } from "react";

import { HighlightedFileCode } from "@/components/palette/HighlightedFileCode";

export function HighlightedCodeEditor({
  value,
  language,
  onChange,
}: {
  value: string;
  language: string;
  onChange: (value: string) => void;
}) {
  const highlightRef = useRef<HTMLDivElement>(null);
  const lineCount = useMemo(() => Math.max(value.split("\n").length, 1), [value]);

  function syncScroll(event: UIEvent<HTMLTextAreaElement>) {
    if (!highlightRef.current) return;
    highlightRef.current.scrollTop = event.currentTarget.scrollTop;
    highlightRef.current.scrollLeft = event.currentTarget.scrollLeft;
  }

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key !== "Tab") return;
    event.preventDefault();
    const element = event.currentTarget;
    const start = element.selectionStart;
    const end = element.selectionEnd;
    const next = `${value.slice(0, start)}  ${value.slice(end)}`;
    onChange(next);
    requestAnimationFrame(() => {
      element.selectionStart = start + 2;
      element.selectionEnd = start + 2;
    });
  }

  return (
    <div className="files-editor-shell">
      <pre className="files-editor-lines" aria-hidden="true">
        {Array.from({ length: lineCount }, (_, index) => index + 1).join("\n")}
      </pre>
      <div ref={highlightRef} className="files-editor-highlight" aria-hidden="true">
        <HighlightedFileCode code={`${value}\n`} language={language} />
      </div>
      <textarea
        className="files-editor"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onScroll={syncScroll}
        onKeyDown={onKeyDown}
        spellCheck={false}
        aria-label="File editor"
      />
    </div>
  );
}
