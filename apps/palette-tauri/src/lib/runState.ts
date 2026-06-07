import type { PaletteResult } from "@/lib/axonClient";
import type { OutputKind } from "@/lib/format";

export type RunState =
  | { kind: "idle" }
  | { kind: "running"; title: string; subtitle: string }
  | {
      kind: "streaming";
      title: string;
      subtitle: string;
      text: string;
      outputKind: OutputKind;
      requestId: string;
      path: string;
      actionLabel: string;
    }
  | {
      kind: "success" | "error";
      title: string;
      subtitle: string;
      text: string;
      outputKind: OutputKind;
      result: PaletteResult;
    };
