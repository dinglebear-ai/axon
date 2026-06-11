import { OutputPanel } from "@/components/palette/OutputPanel";
import { ACTIONS, type PaletteAction } from "@/lib/actions";
import type { RunState } from "@/lib/runState";

const noop = () => {};

export function OperationResultFixture() {
  const cases: Array<{ title: string; action: PaletteAction; run: Extract<RunState, { kind: "success" | "error" }> }> = [
    {
      title: "Structured Error",
      action: actionFor("crawl-status"),
      run: {
        kind: "error",
        title: "Job status failed",
        subtitle: "crawl-status not-a-uuid",
        text: "id must be a UUID",
        outputKind: "code",
        result: {
          ok: false,
          status: 0,
          method: "GET",
          path: "/v1/crawl/not-a-uuid",
          payload: { kind: "missing_param", message: "id must be a UUID", param: "job_id" },
        },
      },
    },
    {
      title: "Scrape Reader",
      action: actionFor("scrape"),
      run: {
        kind: "success",
        title: "Scrape completed",
        subtitle: "https://docs.rs/serde",
        text: "# Serde\n\nSerde is a framework for serializing and deserializing Rust data structures efficiently and generically.",
        outputKind: "markdown",
        result: {
          ok: true,
          status: 200,
          method: "POST",
          path: "/v1/scrape",
          payload: {
            url: "https://docs.rs/serde",
            title: "Serde docs",
            markdown:
              "# Serde\n\nSerde is a framework for serializing and deserializing Rust data structures efficiently and generically.\n\n- Derive support\n- JSON and other formats\n- Zero-copy options\n-\n-\n\n```rust\n#[derive(Serialize, Deserialize)]\nstruct User {\n    id: u64,\n    name: String,\n}\n```\n\n```txt\n-\n```",
          },
        },
      },
    },
    {
      title: "Retrieve Empty",
      action: actionFor("retrieve"),
      run: {
        kind: "success",
        title: "Retrieve completed",
        subtitle: "https://example.com/missing",
        text: "{}",
        outputKind: "code",
        result: {
          ok: true,
          status: 200,
          method: "POST",
          path: "/v1/retrieve",
          payload: { url: "https://example.com/missing", chunks: [] },
        },
      },
    },
    {
      title: "Screenshot Preview",
      action: actionFor("screenshot"),
      run: {
        kind: "success",
        title: "Screenshot completed",
        subtitle: "https://aurora.tootie.tv",
        text: "screenshot captured",
        outputKind: "markdown",
        result: {
          ok: true,
          status: 200,
          method: "POST",
          path: "/v1/screenshot",
          payload: {
            url: "https://aurora.tootie.tv",
            path: "/tmp/axon-aurora-screenshot.png",
            preview_url: previewImage(),
            size_bytes: 48211,
          },
        },
      },
    },
    {
      title: "Watch Empty",
      action: actionFor("watch-list"),
      run: {
        kind: "success",
        title: "Watch list completed",
        subtitle: "scheduled operations",
        text: "[]",
        outputKind: "code",
        result: {
          ok: true,
          status: 200,
          method: "GET",
          path: "/v1/watch",
          payload: { watches: [] },
        },
      },
    },
  ];

  return (
    <main className="operation-fixture">
      <header className="operation-fixture-header">
        <span>Axon Palette</span>
        <h1>Operation Response Fixture</h1>
      </header>
      <div className="operation-fixture-grid">
        {cases.map((item) => (
          <section key={item.title} className="operation-fixture-case">
            <h2>{item.title}</h2>
            <OutputPanel
              active={item.action}
              copied={false}
              outputKind={item.run.outputKind}
              run={item.run}
              onCopy={noop}
              onRetry={noop}
              onFollowUp={noop}
              onHistory={noop}
              onCollapse={noop}
              onTogglePin={noop}
              pinned={false}
            />
          </section>
        ))}
      </div>
    </main>
  );
}

function actionFor(subcommand: string): PaletteAction {
  const action = ACTIONS.find((item) => item.subcommand === subcommand);
  if (!action) throw new Error(`missing fixture action: ${subcommand}`);
  return action;
}

function previewImage(): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="960" height="540" viewBox="0 0 960 540"><rect width="960" height="540" fill="#07131c"/><rect x="72" y="72" width="816" height="96" rx="18" fill="#0f2635" stroke="#1d3d4e"/><rect x="72" y="208" width="384" height="220" rx="18" fill="#102b3d" stroke="#1d3d4e"/><rect x="504" y="208" width="384" height="220" rx="18" fill="#1a2144" stroke="#3b3270"/><circle cx="124" cy="120" r="22" fill="#29b6f6"/><rect x="172" y="103" width="280" height="18" rx="9" fill="#e6f4fb"/><rect x="172" y="132" width="460" height="12" rx="6" fill="#a7bcc9"/><rect x="112" y="254" width="264" height="18" rx="9" fill="#67cbfa"/><rect x="112" y="294" width="300" height="12" rx="6" fill="#a7bcc9"/><rect x="112" y="324" width="236" height="12" rx="6" fill="#a7bcc9"/><rect x="544" y="254" width="220" height="18" rx="9" fill="#f9a8c4"/><rect x="544" y="294" width="284" height="12" rx="6" fill="#a7bcc9"/><rect x="544" y="324" width="244" height="12" rx="6" fill="#a7bcc9"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}
