import type { LucideIcon } from "lucide-react";

import type { PaletteAction } from "@/lib/actions";
import { actionBehavior } from "@/lib/actionRegistry";

export function ActionIcon({ action, selected }: { action: PaletteAction; selected: boolean }) {
  const Icon = actionIcon(action.subcommand);
  return (
    <span className={`action-icon action-icon-${action.tone}${selected ? " action-icon-selected" : ""}`} aria-hidden="true">
      <Icon size={16} strokeWidth={1.65} />
    </span>
  );
}

/** Action-list / command-bar icon for a subcommand. Derived from the registry. */
export function actionIcon(subcommand: string): LucideIcon {
  return actionBehavior(subcommand).actionIcon;
}
