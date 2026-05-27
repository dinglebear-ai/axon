import {
  ACTIONS,
  type PaletteAction,
  acceptsDirectUrl,
  actionInvokedBy,
} from "@/lib/actions";

export type ParsedCommand = { invoked?: PaletteAction; search: string; arg: string };
type OutputRunState =
  | { kind: "idle" }
  | { kind: "running"; title: string; subtitle: string }
  | { kind: "queued" | "success" | "error"; title: string; subtitle: string };

export function focusInput(select = false) {
  window.setTimeout(() => {
    const input = document.querySelector<HTMLInputElement>(".command-input");
    input?.focus();
    if (select) input?.select();
  }, 30);
}

export function parseCommand(raw: string): ParsedCommand {
  const trimmed = raw.trimStart();
  const [token = ""] = trimmed.split(/\s+/);
  const invoked = ACTIONS.find((action) => actionInvokedBy(action, token));
  if (invoked) {
    return { invoked, search: token, arg: trimmed.slice(token.length).trimStart() };
  }
  return { search: trimmed, arg: "" };
}

export function argumentFor(
  action: PaletteAction,
  modeAction: PaletteAction | null,
  parsed: ParsedCommand,
  query: string,
): string {
  if (modeAction?.subcommand === action.subcommand) return query.trim();
  if (parsed.invoked?.subcommand === action.subcommand) return parsed.arg;
  if (looksLikeUrl(parsed.search) && acceptsDirectUrl(action)) return parsed.search;
  return parsed.search;
}

export function validationMessage(action: PaletteAction, argument: string): string {
  if (action.argMode === "none" || action.argMode === "optionalSingle") return "";
  return argument.trim() ? "" : "Argument required";
}

export function actionHint(action: PaletteAction, search: string): string {
  if (acceptsDirectUrl(action) && looksLikeUrl(search)) return "Run URL";
  if (action.argMode === "none") return "Run";
  return "Select";
}

export function argumentPlaceholder(action: PaletteAction): string {
  const example = action.example.replace(new RegExp(`^${action.subcommand}\\s*`, "i"), "").trim();
  return example || action.description;
}

export function looksLikeUrl(value: string): boolean {
  return /^https?:\/\//i.test(value.trim());
}

export function hostLabel(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

export function firstUrl(value: string): string | null {
  return value.match(/https?:\/\/[^\s"')\]}]+/i)?.[0] ?? null;
}

export function runTone(run: OutputRunState): "info" | "success" | "error" | "neutral" {
  if (run.kind === "success") return "success";
  if (run.kind === "error") return "error";
  if (run.kind === "running" || run.kind === "queued") return "info";
  return "neutral";
}

export function outputTitle(run: OutputRunState): string {
  if (run.kind === "idle") return "Ready";
  return run.title;
}

export function outputSubtitle(run: OutputRunState, action: PaletteAction | undefined): string {
  if (run.kind === "idle") return action?.description ?? "No matching action";
  return run.subtitle;
}
