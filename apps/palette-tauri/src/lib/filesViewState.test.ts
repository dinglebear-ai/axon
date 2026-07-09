import { describe, expect, it } from "vitest";

import { clearChecked, createPane } from "./filesModel";
import { createInitialState, filesViewReducer } from "./filesViewState";

describe("createInitialState", () => {
  it("starts with a single left pane, no split, tree width 248", () => {
    const state = createInitialState();
    expect(state.panes).toEqual([createPane("left")]);
    expect(state.activePane).toBe("left");
    expect(state.treeWidth).toBe(248);
    expect(state.checked).toEqual(clearChecked());
  });
});

describe("filesViewReducer — pane lifecycle", () => {
  it("pane/setCwd updates only the targeted pane's cwd", () => {
    const state = createInitialState();
    const next = filesViewReducer(state, { type: "pane/setCwd", pane: "left", cwd: "docs" });
    expect(next.panes[0].cwd).toBe("docs");
  });

  it("pane/select sets the pane's selected entry", () => {
    const state = createInitialState();
    const entry = { name: "a.md", path: "a.md", isDir: false, size: 10 };
    const next = filesViewReducer(state, { type: "pane/select", pane: "left", entry });
    expect(next.panes[0].selected).toEqual(entry);
  });

  it("pane/fileLoading increments loadGen and sets loading state", () => {
    const state = createInitialState();
    const next = filesViewReducer(state, { type: "pane/fileLoading", pane: "left", loadGen: 1 });
    expect(next.panes[0].loadGen).toBe(1);
    expect(next.panes[0].file).toEqual({ kind: "loading" });
  });

  it("pane/fileLoaded is dropped when loadGen is stale", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "pane/fileLoading", pane: "left", loadGen: 1 });
    state = filesViewReducer(state, { type: "pane/fileLoading", pane: "left", loadGen: 2 });
    // A slow resolution for generation 1 arrives after generation 2 already started.
    const stale = filesViewReducer(state, {
      type: "pane/fileLoaded",
      pane: "left",
      loadGen: 1,
      file: { path: "old.md", content: "stale", size: 5 },
    });
    expect(stale.panes[0].file).toEqual({ kind: "loading" });
    expect(stale.panes[0].loadGen).toBe(2);
  });

  it("pane/fileLoaded applies when loadGen matches", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "pane/fileLoading", pane: "left", loadGen: 1 });
    const applied = filesViewReducer(state, {
      type: "pane/fileLoaded",
      pane: "left",
      loadGen: 1,
      file: { path: "a.md", content: "fresh", size: 5 },
    });
    expect(applied.panes[0].file).toEqual({
      kind: "loaded",
      value: { path: "a.md", content: "fresh", size: 5 },
    });
    expect(applied.panes[0].draft).toBe("fresh");
  });
});

describe("filesViewReducer — split view", () => {
  it("split/open adds a right pane seeded with the left pane's cwd", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "pane/setCwd", pane: "left", cwd: "docs" });
    const next = filesViewReducer(state, { type: "split/open" });
    expect(next.panes).toHaveLength(2);
    expect(next.panes[1]?.id).toBe("right");
    expect(next.panes[1]?.cwd).toBe("docs");
  });

  it("split/open is idempotent when already split", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "split/open" });
    const next = filesViewReducer(state, { type: "split/open" });
    expect(next.panes).toHaveLength(2);
  });

  it("split/close drops the right pane and resets active pane to left", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "split/open" });
    state = filesViewReducer(state, { type: "pane/setActive", pane: "right" });
    const next = filesViewReducer(state, { type: "split/close" });
    expect(next.panes).toHaveLength(1);
    expect(next.activePane).toBe("left");
  });

  it("pane/setActive only applies when split is open", () => {
    const state = createInitialState();
    const next = filesViewReducer(state, { type: "pane/setActive", pane: "right" });
    expect(next.activePane).toBe("left");
  });
});

describe("filesViewReducer — tree width", () => {
  it("treeWidth/set clamps to [180, 460]", () => {
    const state = createInitialState();
    expect(filesViewReducer(state, { type: "treeWidth/set", width: 50 }).treeWidth).toBe(180);
    expect(filesViewReducer(state, { type: "treeWidth/set", width: 900 }).treeWidth).toBe(460);
    expect(filesViewReducer(state, { type: "treeWidth/set", width: 300 }).treeWidth).toBe(300);
  });
});

describe("filesViewReducer — bulk checked set", () => {
  it("checked/toggle and checked/clear route through the shared helpers", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "checked/toggle", path: "a.md" });
    expect(state.checked.has("a.md")).toBe(true);
    state = filesViewReducer(state, { type: "checked/clear" });
    expect(state.checked.size).toBe(0);
  });
});
