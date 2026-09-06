import type { LabbyToolDescriptor } from "../clients/labbyClient";
import { parseRawArguments } from "./schemaForm";

export interface SnippetDraft {
  name: string;
  description: string;
  body: string;
  paramsText: string;
  baseBody: string | null;
}

export const EMPTY_SNIPPET_DRAFT: SnippetDraft = {
  name: "",
  description: "",
  body: "async (input) => ({ ok: true })",
  paramsText: "{}",
  baseBody: null,
};

export function insertToolCall(
  body: string,
  descriptor: LabbyToolDescriptor,
  argumentsText = "{}",
): string {
  const args = parseRawArguments(argumentsText);
  const upstream = safeIdentifier(descriptor.upstream ?? descriptor.source);
  const tool = safeIdentifier(descriptor.tool ?? descriptor.label);
  const call = `await codemode.${upstream}.${tool}(${JSON.stringify(args, null, 2)})`;
  if (/async\s*\([^)]*\)\s*=>\s*\(\{\s*ok:\s*true\s*\}\)/.test(body))
    return `async (input) => {\n  const result = ${indent(call, 2)};\n  return { ok: true, result };\n}`;
  return `${body.trimEnd()}\n\n// ${descriptor.id} @ ${descriptor.catalogRevision}\n${call}`;
}

function safeIdentifier(value: string): string {
  const identifier = value.replace(/[^A-Za-z0-9_$]/g, "_");
  return /^[A-Za-z_$]/.test(identifier) ? identifier : `_${identifier}`;
}

function indent(value: string, spaces: number): string {
  return value.replace(/\n/g, `\n${" ".repeat(spaces)}`);
}

export function parseSnippetParams(value: string): Record<string, unknown> {
  return parseRawArguments(value);
}

export function hasUnsavedSnippetChanges(draft: SnippetDraft): boolean {
  return draft.baseBody !== null && draft.body !== draft.baseBody;
}
