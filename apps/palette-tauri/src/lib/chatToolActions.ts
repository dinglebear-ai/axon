import type { PaletteAction } from "@/lib/actions";
import type { PaletteResult } from "@/lib/axonClient";
import { actionDisplayMeta } from "@/lib/actionMeta";
import { formatPayload } from "@/lib/format";
import { asyncJobStart } from "@/lib/paletteView";

function headingFor(action: PaletteAction, result: PaletteResult): string {
  const label = actionDisplayMeta(action).label;
  if (!result.ok) return `${label} failed`;
  const job = asyncJobStart(result.payload);
  if (job) return `${label} queued`;
  return `${label} completed`;
}

export function chatToolMessage(action: PaletteAction, argument: string, result: PaletteResult): string {
  const lines = [
    `### ${headingFor(action, result)}`,
    "",
    `- Command: \`/${action.subcommand}${argument ? ` ${argument}` : ""}\``,
    `- Request: \`${result.method} ${result.path}\``,
    `- HTTP: ${result.status}`,
  ];
  const job = asyncJobStart(result.payload);
  if (job) {
    lines.push(`- Job id: \`${job.jobId}\``, `- Status: ${job.status}`);
  }
  const formatted = formatPayload(action.subcommand, result.payload).trim();
  if (formatted) lines.push("", formatted);
  return lines.join("\n");
}
