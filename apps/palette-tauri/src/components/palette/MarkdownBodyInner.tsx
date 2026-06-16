import { Streamdown } from "streamdown";

import { STREAMDOWN_CODE_THEMES, STREAMDOWN_PLUGINS, STREAMDOWN_REHYPE_PLUGINS } from "@/lib/streamdownConfig";

// The heavy markdown renderer. Imports streamdown (which pulls in the shiki code
// plugin), so this module is split into its own chunk and loaded lazily by
// MarkdownBody.tsx — see P-H1. Every <Streamdown> render in the palette flows
// through here, sharing the hardened rehype pipeline (S-M1/S-M2/S-L1).
export function MarkdownBodyInner({ children }: { children: string }) {
  return (
    <Streamdown plugins={STREAMDOWN_PLUGINS} rehypePlugins={STREAMDOWN_REHYPE_PLUGINS} shikiTheme={STREAMDOWN_CODE_THEMES}>
      {children}
    </Streamdown>
  );
}

export default MarkdownBodyInner;
