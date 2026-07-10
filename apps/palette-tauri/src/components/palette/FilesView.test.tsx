// @vitest-environment jsdom
//
// Behavioral tests for FilesView: the browser-dev fallback message when
// `isTauriRuntime` is false, directory listing + navigation + file preview
// against a mocked Tauri fs bridge, and the real ingest wiring — proving it
// dispatches through `axon_http_request` with the same shape the `embed`
// action would build (POST /v1/sources with the file's absolute path).

import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const runtimeState = vi.hoisted(() => ({ isTauriRuntime: true }));
const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/invoke", () => ({
  get isTauriRuntime() {
    return runtimeState.isTauriRuntime;
  },
  invoke: invokeMock,
}));

import { createAxonClient, type PaletteConfig } from "@/lib/axonClient";
import type { DirListing, FileContents } from "@/lib/filesModel";
import { FilesView } from "./FilesView";

const config: PaletteConfig = {
  serverUrl: "http://127.0.0.1:8001",
  token: null,
  shortcut: "Ctrl+Shift+Space",
  collection: "axon",
  resultLimit: 10,
  theme: "dark",
  hideOnBlur: false,
};
const client = createAxonClient(config);

const rootListing: DirListing = {
  path: "",
  root: "/home/user",
  entries: [
    { name: "notes", path: "notes", isDir: true, size: 0, modifiedUnix: null },
    { name: "README.md", path: "README.md", isDir: false, size: 128, modifiedUnix: 1_700_000_000 },
    { name: "photo.png", path: "photo.png", isDir: false, size: 2048, modifiedUnix: null },
  ],
};

const readmeContents: FileContents = {
  path: "README.md",
  content: "# Hello\n\nSome docs.",
  size: 128,
};

beforeEach(() => {
  runtimeState.isTauriRuntime = true;
  invokeMock.mockReset();
});

afterEach(() => {
  cleanup();
});

describe("FilesView — browser dev fallback", () => {
  it("shows a requires-desktop-app message and never calls invoke", () => {
    runtimeState.isTauriRuntime = false;
    render(<FilesView client={null} config={null} />);

    expect(screen.getByText(/requires the desktop app/i)).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe("FilesView — directory listing", () => {
  it("lists directory entries and lets the user open a file", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);

    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    expect(screen.getByText("notes")).toBeInTheDocument();
    expect(screen.getByText("photo.png")).toBeInTheDocument();
    expect(screen.getByText("Select a file")).toBeInTheDocument();

    fireEvent.click(screen.getByText("README.md"));

    await waitFor(() => expect(screen.getByText(/# Hello/)).toBeInTheDocument());
    expect(invokeMock).toHaveBeenCalledWith("files_read_file", { path: "README.md" });
  });

  it("navigates into a subdirectory and lists its contents", async () => {
    const nestedListing: DirListing = {
      path: "notes",
      root: "/home/user",
      entries: [
        { name: "todo.txt", path: "notes/todo.txt", isDir: false, size: 12, modifiedUnix: null },
      ],
    };
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "files_list_dir") {
        return Promise.resolve(args?.path === "notes" ? nestedListing : rootListing);
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("notes")).toBeInTheDocument());

    fireEvent.click(screen.getByText("notes"));

    await waitFor(() => expect(screen.getByText("todo.txt")).toBeInTheDocument());
    expect(invokeMock).toHaveBeenCalledWith("files_list_dir", { path: "notes" });
  });

  it("shows an error message when listing fails", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir")
        return Promise.reject(new Error("path escapes the allowed files root"));
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);

    await waitFor(() =>
      expect(screen.getByText(/path escapes the allowed files root/)).toBeInTheDocument(),
    );
  });
});

describe("FilesView — real ingest wiring", () => {
  it("ingests the selected file via the real embed request shape", async () => {
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      if (command === "axon_http_request") {
        const request = args?.request as { method: string; path: string; body: unknown };
        expect(request.method).toBe("POST");
        expect(request.path).toBe("/v1/sources");
        expect(request.body).toEqual({ source: "/home/user/README.md", collection: "axon" });
        return Promise.resolve({
          ok: true,
          status: 200,
          path: "/v1/sources",
          method: "POST",
          payload: { job_id: "abc" },
        });
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /ingest/i })).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: /ingest/i }));

    await waitFor(() => expect(screen.getByText(/Queued for ingest/i)).toBeInTheDocument());
    expect(invokeMock).toHaveBeenCalledWith(
      "axon_http_request",
      expect.objectContaining({
        request: expect.objectContaining({ method: "POST", path: "/v1/sources" }),
      }),
    );
  });

  it("does not offer ingest for a non-ingestable (binary) file", async () => {
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file" && args?.path === "photo.png") {
        return Promise.reject(new Error("file is not valid UTF-8 text"));
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("photo.png")).toBeInTheDocument());
    fireEvent.click(screen.getByText("photo.png"));

    await waitFor(() => expect(screen.getByText(/not valid UTF-8/)).toBeInTheDocument());
    expect(screen.queryByRole("button", { name: /ingest/i })).not.toBeInTheDocument();
  });

  it("shows a failure message when the ingest request fails", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      if (command === "axon_http_request") {
        return Promise.resolve({
          ok: false,
          status: 500,
          path: "/v1/sources",
          method: "POST",
          payload: { message: "TEI unreachable" },
        });
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /ingest/i })).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: /ingest/i }));

    await waitFor(() => expect(screen.getByText(/TEI unreachable/)).toBeInTheDocument());
  });
});

describe("FilesView — edit and save", () => {
  it("edits and saves a file via files_write_file", async () => {
    const savedContents: FileContents = { path: "README.md", content: "# Edited", size: 8 };
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      if (command === "files_write_file") {
        expect(args?.path).toBe("README.md");
        return Promise.resolve(savedContents);
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /^edit$/i })).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const textarea = screen.getByRole("textbox");
    fireEvent.change(textarea, { target: { value: "# Edited" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("files_write_file", {
        path: "README.md",
        content: "# Edited",
      }),
    );
  });
});

describe("FilesView — split view", () => {
  beforeEach(() => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      throw new Error(`unexpected command: ${command}`);
    });
  });

  it("renders a single pane by default", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    expect(screen.queryAllByRole("listbox", { name: /directory entries/i })).toHaveLength(1);
  });

  it("shows a 'Split view' icon control that opens a second pane", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    const splitButton = screen.getByRole("button", { name: /split view/i });
    await userEvent.click(splitButton);
    await waitFor(() =>
      expect(screen.getAllByRole("listbox", { name: /directory entries/i })).toHaveLength(2),
    );
  });

  it("closes the second pane when 'Close split' is clicked", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: /split view/i }));
    const closeSplit = await screen.findByRole("button", { name: /close split/i });
    await userEvent.click(closeSplit);
    expect(screen.getAllByRole("listbox", { name: /directory entries/i })).toHaveLength(1);
  });

  it("renders a resize handle for the tree column", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    expect(screen.getByRole("separator", { name: /resize file tree/i })).toBeInTheDocument();
  });

  it("dragging the tree-resize handle updates the tree column width", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    const handle = screen.getByRole("separator", { name: /resize file tree/i });
    const [tree] = screen.getAllByRole("listbox", { name: /directory entries/i });
    fireEvent.mouseDown(handle, { clientX: 248 });
    fireEvent.mouseMove(window, { clientX: 300 });
    fireEvent.mouseUp(window);
    expect(tree).toHaveStyle({ width: "300px" });
  });

  it("each pane tracks its own selected file independently", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: /split view/i }));
    await waitFor(() =>
      expect(screen.getAllByRole("listbox", { name: /directory entries/i })).toHaveLength(2),
    );
    const [leftTree, rightTree] = screen.getAllByRole("listbox", { name: /directory entries/i });
    const leftEntry = within(leftTree).getByText("README.md");
    await userEvent.click(leftEntry);
    const rightPreview = rightTree.closest(".files-body")?.querySelector(".files-preview");
    expect(rightPreview).toHaveTextContent(/select a file/i);
  });

  it("clicking inside the right pane makes it active", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: /split view/i }));
    await waitFor(() =>
      expect(screen.getAllByRole("listbox", { name: /directory entries/i })).toHaveLength(2),
    );
    const [, rightTree] = screen.getAllByRole("listbox", { name: /directory entries/i });
    await userEvent.click(rightTree);
    const rightEntry = within(rightTree).getByText("README.md");
    await userEvent.click(rightEntry);
    const [leftPreview] = screen
      .getAllByRole("listbox", { name: /directory entries/i })
      .map((tree) => tree.closest(".files-body")?.querySelector(".files-preview"));
    expect(leftPreview).toHaveTextContent(/select a file/i);
  });
});

describe("FilesView — bulk selection and ingest", () => {
  beforeEach(() => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      if (command === "axon_http_request") {
        return Promise.resolve({
          ok: true,
          status: 200,
          path: "/v1/sources",
          method: "POST",
          payload: { job_id: "abc" },
        });
      }
      throw new Error(`unexpected command: ${command}`);
    });
  });

  it("shows a checkbox on each file row with the generic bulk-ingest label", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    expect(checkboxes.length).toBeGreaterThan(0);
  });

  it("shows no bulk bar when nothing is checked", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    expect(screen.queryByText(/selected/i)).not.toBeInTheDocument();
  });

  it("shows 'N selected' and 'Ingest all' after checking files", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    await userEvent.click(checkboxes[0]);
    await userEvent.click(checkboxes[1]);
    expect(screen.getByText("2 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /ingest all/i })).toBeInTheDocument();
  });

  it("queues one sequential ingest call per checked file when 'Ingest all' is clicked", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    await userEvent.click(checkboxes[0]);
    await userEvent.click(checkboxes[1]);
    await userEvent.click(screen.getByRole("button", { name: /ingest all/i }));
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter((call) => call[0] === "axon_http_request")).toHaveLength(2),
    );
  });

  it("shows a per-item progress line while ingesting", async () => {
    const resolvers: Array<() => void> = [];
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      if (command === "axon_http_request") {
        return new Promise((resolve) => {
          resolvers.push(() =>
            resolve({ ok: true, status: 200, path: "/v1/sources", method: "POST", payload: {} }),
          );
        });
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    await userEvent.click(checkboxes[0]);
    await userEvent.click(checkboxes[1]);
    await userEvent.click(screen.getByRole("button", { name: /ingest all/i }));
    await waitFor(() => expect(screen.getByText(/ingesting 1\/2/i)).toBeInTheDocument());
    resolvers[0]?.();
  });

  it("shows a Cancel affordance while running and stops after the in-flight item", async () => {
    const resolvers: Array<() => void> = [];
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      if (command === "axon_http_request") {
        return new Promise((resolve) => {
          resolvers.push(() =>
            resolve({ ok: true, status: 200, path: "/v1/sources", method: "POST", payload: {} }),
          );
        });
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    await userEvent.click(checkboxes[0]);
    await userEvent.click(checkboxes[1]);
    await userEvent.click(screen.getByRole("button", { name: /ingest all/i }));
    await waitFor(() => expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: /cancel/i }));
    resolvers[0]?.();
    await waitFor(() => expect(screen.getByText(/cancelled after 1\/2/i)).toBeInTheDocument());
    expect(resolvers).toHaveLength(1);
  });

  it("clears the checked set after a successful bulk ingest", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    await userEvent.click(checkboxes[0]);
    await userEvent.click(screen.getByRole("button", { name: /ingest all/i }));
    await waitFor(() => expect(screen.queryByText(/selected/i)).not.toBeInTheDocument());
  });
});

describe("FilesView — AI-assisted edit proposal", () => {
  beforeEach(() => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      throw new Error(`unexpected command: ${command}`);
    });
  });

  it("shows an 'Edit with the model' button separate from the manual Edit button", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() => expect(screen.getByRole("button", { name: /^edit$/i })).toBeInTheDocument());
    expect(screen.getByRole("button", { name: /edit with the model/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument();
  });

  it("opens an inline instruction prompt on sparkle click", async () => {
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /edit with the model/i })).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: /edit with the model/i }));
    expect(screen.getByPlaceholderText(/describe the edit/i)).toBeInTheDocument();
  });

  it("submits the instruction via chat and shows a proposed diff with Deny/Approve", async () => {
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      if (command === "axon_http_request") {
        const request = args?.request as { path: string; body: { message: string } };
        expect(request.path).toBe("/v1/chat");
        expect(request.body.message).toContain("rewrite the intro");
        expect(request.body.message).toContain(readmeContents.content);
        return Promise.resolve({
          ok: true,
          status: 200,
          path: "/v1/chat",
          method: "POST",
          payload: { answer: "# Title\n\nrewritten body", message: "" },
        });
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /edit with the model/i })).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: /edit with the model/i }));
    await userEvent.type(screen.getByPlaceholderText(/describe the edit/i), "rewrite the intro{Enter}");
    expect(await screen.findByText(/proposed edit/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /deny/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /approve/i })).toBeInTheDocument();
  });

  it("Deny discards the proposal without writing", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      if (command === "axon_http_request") {
        return Promise.resolve({
          ok: true,
          status: 200,
          path: "/v1/chat",
          method: "POST",
          payload: { answer: "rewritten", message: "" },
        });
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /edit with the model/i })).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: /edit with the model/i }));
    await userEvent.type(screen.getByPlaceholderText(/describe the edit/i), "rewrite{Enter}");
    await screen.findByText(/proposed edit/i);
    await userEvent.click(screen.getByRole("button", { name: /deny/i }));
    expect(screen.queryByText(/proposed edit/i)).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("files_write_file", expect.anything());
  });

  it("Approve writes the proposed content via files_write_file", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      if (command === "axon_http_request") {
        return Promise.resolve({
          ok: true,
          status: 200,
          path: "/v1/chat",
          method: "POST",
          payload: { answer: "rewritten", message: "" },
        });
      }
      if (command === "files_write_file") {
        return Promise.resolve({ path: "README.md", content: "rewritten", size: 9 });
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /edit with the model/i })).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: /edit with the model/i }));
    await userEvent.type(screen.getByPlaceholderText(/describe the edit/i), "rewrite{Enter}");
    await screen.findByText(/proposed edit/i);
    await userEvent.click(screen.getByRole("button", { name: /approve/i }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("files_write_file", {
        path: "README.md",
        content: "rewritten",
      }),
    );
  });

  it("Approve fails with a clear error when the file changed on disk since the proposal", async () => {
    let readCount = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") {
        readCount += 1;
        // First read is the initial file-open; second is Approve's
        // disk-staleness re-check, which observes a since-changed file.
        return Promise.resolve(
          readCount === 1 ? readmeContents : { ...readmeContents, content: "changed elsewhere" },
        );
      }
      if (command === "axon_http_request") {
        return Promise.resolve({
          ok: true,
          status: 200,
          path: "/v1/chat",
          method: "POST",
          payload: { answer: "rewritten", message: "" },
        });
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /edit with the model/i })).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: /edit with the model/i }));
    await userEvent.type(screen.getByPlaceholderText(/describe the edit/i), "rewrite{Enter}");
    await screen.findByText(/proposed edit/i);
    await userEvent.click(screen.getByRole("button", { name: /approve/i }));
    expect(await screen.findByText(/changed on disk/i)).toBeInTheDocument();
  });
});

describe("FilesView — SFTP toolbar affordance", () => {
  it("shows a 'Connect SFTP' control in the toolbar", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "sftp_list_known_hosts") return Promise.resolve([]);
      throw new Error(`unexpected command: ${command}`);
    });
    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: /connect sftp/i })).toBeInTheDocument();
  });
});

describe("FilesView — SFTP entries gate manual and AI-assisted edit", () => {
  it("renders neither the Edit nor the Edit-with-the-model button for an SFTP-origin file, but both for local", async () => {
    const connectedProfile = {
      id: "conn-1",
      label: "prod",
      host: "example.com",
      port: 22,
      username: "deploy",
      privateKeyPath: "/home/me/.ssh/id_ed25519",
    };
    const sftpListing = {
      path: "",
      entries: [{ name: "remote.md", path: "remote.md", isDir: false, size: 40, modifiedUnix: null }],
    };
    const sftpFile = { path: "remote.md", content: "# remote" };

    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      if (command === "sftp_list_known_hosts") return Promise.resolve([]);
      if (command === "sftp_connect") {
        return Promise.resolve({ kind: "connected", connectionId: "sftp-conn-1" });
      }
      if (command === "sftp_list_dir") return Promise.resolve(sftpListing);
      if (command === "sftp_read_file") return Promise.resolve(sftpFile);
      throw new Error(`unexpected command: ${command}, args: ${JSON.stringify(args)}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());

    // Local file: both Edit and Edit-with-the-model are present.
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() => expect(screen.getByRole("button", { name: /^edit$/i })).toBeInTheDocument());
    expect(screen.getByRole("button", { name: /edit with the model/i })).toBeInTheDocument();

    // Connect an SFTP profile via the toolbar control + dialog.
    await userEvent.click(screen.getByRole("button", { name: /connect sftp/i }));
    await userEvent.type(screen.getByLabelText(/^label$/i), connectedProfile.label);
    await userEvent.type(screen.getByLabelText(/^host$/i), connectedProfile.host);
    await userEvent.type(screen.getByLabelText(/^username$/i), connectedProfile.username);
    await userEvent.type(screen.getByLabelText(/private key path/i), connectedProfile.privateKeyPath);
    await userEvent.click(screen.getByRole("button", { name: /^connect$/i }));

    await waitFor(() => expect(screen.getByText("remote.md")).toBeInTheDocument());
    fireEvent.click(screen.getByText("remote.md"));
    await waitFor(() => expect(screen.getByText(/# remote/)).toBeInTheDocument());

    expect(screen.queryByRole("button", { name: /^edit$/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /edit with the model/i })).not.toBeInTheDocument();
  });
});

describe("FilesView — SFTP open race (P2 #6 regression)", () => {
  it("keeps a local file's content when it's opened before a slower SFTP read resolves", async () => {
    const sftpListing = {
      path: "",
      entries: [{ name: "remote.md", path: "remote.md", isDir: false, size: 40, modifiedUnix: null }],
    };
    // The SFTP read never resolves during this test — it stands in for "a
    // slow remote read still in flight when the user opens a different local
    // file." If openSftpEntry captured `loadGen` after the await (the bug),
    // this promise resolving later would still pass the (now-current) gen
    // check and clobber the local file that was opened in the meantime.
    let resolveSftpRead: (value: { path: string; content: string }) => void = () => {};
    const sftpReadPromise = new Promise<{ path: string; content: string }>((resolve) => {
      resolveSftpRead = resolve;
    });

    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "files_list_dir") return Promise.resolve(rootListing);
      if (command === "files_read_file") return Promise.resolve(readmeContents);
      if (command === "sftp_list_known_hosts") return Promise.resolve([]);
      if (command === "sftp_connect") {
        return Promise.resolve({ kind: "connected", connectionId: "sftp-conn-1" });
      }
      if (command === "sftp_list_dir") return Promise.resolve(sftpListing);
      if (command === "sftp_read_file") return sftpReadPromise;
      if (command === "save_palette_settings") return Promise.resolve(args?.settings);
      throw new Error(`unexpected command: ${command}, args: ${JSON.stringify(args)}`);
    });

    render(<FilesView client={client} config={config} />);
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());

    await userEvent.click(screen.getByRole("button", { name: /connect sftp/i }));
    await userEvent.type(screen.getByLabelText(/^host$/i), "example.com");
    await userEvent.type(screen.getByLabelText(/^username$/i), "deploy");
    await userEvent.type(screen.getByLabelText(/private key path/i), "/home/me/.ssh/id_ed25519");
    await userEvent.click(screen.getByRole("button", { name: /^connect$/i }));
    await waitFor(() => expect(screen.getByText("remote.md")).toBeInTheDocument());

    // Start the (slow) SFTP read, then immediately open a local file before
    // it resolves.
    fireEvent.click(screen.getByText("remote.md"));
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() => expect(screen.getByText(/# Hello/)).toBeInTheDocument());

    // Now let the stale SFTP read resolve — it must NOT overwrite the local
    // file that was opened after it started.
    resolveSftpRead({ path: "remote.md", content: "# remote (stale)" });
    await Promise.resolve();
    await Promise.resolve();

    expect(screen.getByText(/# Hello/)).toBeInTheDocument();
    expect(screen.queryByText(/# remote/)).not.toBeInTheDocument();
  });
});
