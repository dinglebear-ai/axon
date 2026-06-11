import { AlertTriangle, Route, ServerCrash } from "lucide-react";

import type { PaletteResult } from "@/lib/axonClient";
import { arrField, strField, unwrapPayload } from "@/lib/payload";

interface ErrorResultViewProps {
  result: PaletteResult;
  text: string;
}

export function ErrorResultView({ result, text }: ErrorResultViewProps) {
  const payload = unwrapPayload(result.payload);
  const message = errorMessage(payload, text);
  const kind = strField(payload, "kind") ?? strField(payload, "code") ?? (result.status ? "request_failed" : "client_error");
  const details = detailRows(payload);

  return (
    <div className="output-body operation-view aurora-scrollbar">
      <section className="operation-hero operation-hero-error">
        <div className="operation-hero-icon">
          <ServerCrash size={16} />
        </div>
        <div className="operation-hero-main">
          <h3>{humanizeKind(kind)}</h3>
          <div className="operation-metrics">
            <span>
              <strong>{result.status || "local"}</strong>
              status
            </span>
            <span>
              <strong>{result.method}</strong>
              method
            </span>
            <span>
              <strong>{result.path}</strong>
              path
            </span>
          </div>
        </div>
      </section>

      <section className="operation-section">
        <div className="operation-error-card">
          <AlertTriangle size={16} />
          <div>
            <strong>{message}</strong>
            <span>{hintFor(kind, payload)}</span>
          </div>
        </div>
      </section>

      {details.length > 0 ? (
        <section className="operation-section">
          <div className="operation-detail-card">
            {details.map(([label, value]) => (
              <div key={label} className="operation-detail-line">
                <span>{label}</span>
                <strong className={isMonoLabel(label) ? "operation-mono" : undefined}>{value}</strong>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      <section className="operation-section">
        <div className="operation-route-card">
          <Route size={14} />
          <code>
            {result.method} {result.path}
          </code>
        </div>
      </section>
    </div>
  );
}

function errorMessage(payload: Record<string, unknown>, text: string): string {
  return strField(payload, "message") ?? strField(payload, "error") ?? strField(payload, "detail") ?? text.trim() ?? "The action failed.";
}

function detailRows(payload: Record<string, unknown>): Array<[string, string]> {
  const valid = arrField(payload, "valid").filter((item): item is string => typeof item === "string");
  const rows: Array<[string, string]> = [];
  for (const key of ["param", "hint", "retry_after_ms", "request_id", "url", "job_id"]) {
    const value = payload[key];
    if (value !== undefined && value !== null) rows.push([labelize(key), String(value)]);
  }
  if (valid.length > 0) rows.push(["Valid options", valid.slice(0, 8).join(", ")]);
  return rows;
}

function hintFor(kind: string, payload: Record<string, unknown>): string {
  const hint = strField(payload, "hint");
  if (hint) return hint;
  switch (kind) {
    case "missing_param":
    case "client_error":
      return "Check the command argument and run the action again.";
    case "unknown_action":
      return "The palette routed to an action the server does not recognize.";
    case "auth_failed":
    case "unauthorized":
      return "Check the Axon token in palette settings.";
    case "rate_limited":
      return "Axon asked us to slow down before retrying.";
    default:
      return "The request reached the failure path; the route and details are preserved here.";
  }
}

function humanizeKind(kind: string): string {
  return labelize(kind || "request_failed");
}

function labelize(value: string): string {
  return value
    .replace(/_/g, " ")
    .split(" ")
    .filter(Boolean)
    .map((part) => (part.length <= 2 ? part.toUpperCase() : part.charAt(0).toUpperCase() + part.slice(1)))
    .join(" ");
}

function isMonoLabel(label: string): boolean {
  const lower = label.toLowerCase();
  return lower.includes("id") || lower.includes("url") || lower.includes("request");
}
