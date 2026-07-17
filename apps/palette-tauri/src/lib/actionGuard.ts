import type { PaletteAction } from "./actions";

export interface ActionGuard {
  label: "Confirm" | "Review";
  tone: "warn" | "danger";
  message: string;
}

export interface PendingActionConfirmation {
  subcommand: string;
  argument: string;
}

const DESTRUCTIVE_SUBCOMMANDS = new Set([
  "jobs-clear",
]);

const STATEFUL_SUBCOMMANDS = new Set([
  "jobs-cancel",
  "jobs-cleanup",
  "watch-create",
  "watch-run",
]);

export function actionGuard(action: PaletteAction): ActionGuard | null {
  if (DESTRUCTIVE_SUBCOMMANDS.has(action.subcommand)) {
    return {
      label: "Confirm",
      tone: "danger",
      message: "Review before running; this can delete or rewrite stored state.",
    };
  }
  if (STATEFUL_SUBCOMMANDS.has(action.subcommand)) {
    return {
      label: "Review",
      tone: "warn",
      message: "Review before running; this changes queued or scheduled work.",
    };
  }
  return null;
}

export function actionNeedsConfirmation(action: PaletteAction): boolean {
  return actionGuard(action) !== null;
}

export function confirmationFor(action: PaletteAction, argument: string): PendingActionConfirmation {
  return { subcommand: action.subcommand, argument: normalizedArgument(argument) };
}

export function actionConfirmationArmed(
  pending: PendingActionConfirmation | null,
  action: PaletteAction,
  argument: string,
): boolean {
  return (
    pending?.subcommand === action.subcommand &&
    pending.argument === normalizedArgument(argument)
  );
}

export function actionConfirmationMessage(action: PaletteAction, armed: boolean): string {
  const guard = actionGuard(action);
  if (!guard) return "";
  return armed ? "Confirmation armed. Press Enter again to run." : guard.message;
}

function normalizedArgument(argument: string): string {
  return argument.trim();
}
