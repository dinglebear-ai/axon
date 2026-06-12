import { describe, expect, it } from "vitest";

import { resolvePaletteWindowSize } from "./useWindowChrome";

describe("resolvePaletteWindowSize", () => {
  it("uses a shorter result window for expanded crawl jobs", () => {
    expect(
      resolvePaletteWindowSize(
        {
          jobExpanded: true,
          jobMinimized: false,
          settingsOpen: false,
          historyOpen: false,
          showResultsLayout: true,
          showContent: true,
          filteredLength: 0,
        },
        { width: 2560, height: 1440 },
        () => 468,
      ),
    ).toEqual({ width: 1280, height: 620 });
  });
});
