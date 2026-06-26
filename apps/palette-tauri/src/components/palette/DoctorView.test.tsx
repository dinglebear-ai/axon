// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { DoctorView } from "./DoctorView";

afterEach(cleanup);

const report = {
  all_ok: false,
  pending_jobs: 2,
  services: {
    qdrant: { ok: true, configured_url: "http://q:6333", latency_ms: 4 },
    tei: { ok: false, configured_url: "http://t:80", error: "connection refused" },
  },
  capabilities: [
    { tier: "tier_1_crawl_retrieve", available: true, impact: [], remedies: [] },
    {
      tier: "tier_2_embedding",
      available: false,
      impact: ["embed and semantic search require TEI"],
      remedies: ["start TEI or configure TEI_URL"],
    },
  ],
  recommendations: ["run `axon serve` only to expose the HTTP API."],
  pipelines: { crawl: true, embed: true },
};

describe("DoctorView", () => {
  it("renders degraded summary, per-service health, latency, and pending jobs", () => {
    render(<DoctorView payload={report} />);
    expect(screen.getByText("Degraded")).toBeInTheDocument();
    expect(screen.getByText("2 pending jobs")).toBeInTheDocument();
    expect(screen.getByText("qdrant")).toBeInTheDocument();
    expect(screen.getByText("4ms")).toBeInTheDocument();
    expect(screen.getByText("down")).toBeInTheDocument();
  });

  it("renders capability tiers with impact + remedy for unavailable ones", () => {
    render(<DoctorView payload={report} />);
    expect(screen.getByText(/embed and semantic search require TEI/)).toBeInTheDocument();
    expect(screen.getByText(/start TEI or configure TEI_URL/)).toBeInTheDocument();
  });

  it("renders recommendations", () => {
    render(<DoctorView payload={report} />);
    expect(screen.getByText(/expose the HTTP API/)).toBeInTheDocument();
  });
});
