import type { ThemeInput } from "streamdown";

import { limitedCode } from "@/lib/limitedStreamdownCode";

export const STREAMDOWN_PLUGINS = { code: limitedCode };
export const STREAMDOWN_CODE_THEMES: [ThemeInput, ThemeInput] = ["one-dark-pro", "one-dark-pro"];
