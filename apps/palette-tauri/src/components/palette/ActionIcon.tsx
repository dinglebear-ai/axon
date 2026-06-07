import {
  Activity,
  BarChart3,
  BookOpen,
  Bot,
  Boxes,
  Braces,
  Camera,
  Database,
  FileDown,
  GitCompare,
  Globe,
  HelpCircle,
  Layers,
  Map as MapIcon,
  PackageOpen,
  SearchCheck,
  Sparkles,
  Stethoscope,
  Workflow,
} from "lucide-react";

import type { PaletteAction } from "@/lib/actions";

export function ActionIcon({ action, selected }: { action: PaletteAction; selected: boolean }) {
  const Icon = actionIcon(action.subcommand);
  return (
    <span className={`action-icon action-icon-${action.tone}${selected ? " action-icon-selected" : ""}`} aria-hidden="true">
      <Icon size={16} strokeWidth={1.65} />
    </span>
  );
}

export function actionIcon(subcommand: string) {
  switch (subcommand) {
    case "scrape":
      return FileDown;
    case "crawl":
      return Workflow;
    case "map":
      return MapIcon;
    case "summarize":
      return BookOpen;
    case "ask":
    case "chat":
      return Bot;
    case "query":
      return SearchCheck;
    case "retrieve":
      return Database;
    case "suggest":
      return Sparkles;
    case "evaluate":
      return BarChart3;
    case "search":
    case "research":
      return Globe;
    case "embed":
      return Layers;
    case "extract":
      return Braces;
    case "ingest":
      return PackageOpen;
    case "status":
      return Activity;
    case "sources":
      return Boxes;
    case "domains":
      return Database;
    case "stats":
      return BarChart3;
    case "doctor":
      return Stethoscope;
    case "brand":
      return Sparkles;
    case "diff":
      return GitCompare;
    case "screenshot":
      return Camera;
    default:
      return HelpCircle;
  }
}
