// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BackendProfile } from "@/lib/backendProfiles/model";
import { ArtifactWorkspace } from "./ArtifactWorkspace";

const artifactAction = vi.fn();
vi.mock("@/lib/clients/labbyClient", async (original) => {
  const actual = await original<typeof import("@/lib/clients/labbyClient")>();
  return {
    ...actual,
    LabbyClient: class {
      identity() {
        return Promise.resolve({
          payload: {
            contract_version: "1.0.0",
            product: "labby",
            server_id: "labby-test",
            product_version: "1",
            api_version: { major: 1, minor: 0 },
            capabilities: ["skills"],
            auth: { modes: ["bearer"], credential_generation: "g1" },
            streams: { transport: "none", resume: "none" },
          },
        });
      }
      artifactAction = artifactAction;
    },
  };
});

const profile: BackendProfile = {
  id: "labby-1",
  label: "Labby",
  product: "labby",
  origin: "https://labby.test",
  credentialHandle: "secret",
  pinnedServerId: null,
  acceptedApiMajor: 1,
};

describe("ArtifactWorkspace", () => {
  beforeEach(() => {
    artifactAction.mockReset().mockImplementation((_family: string, action: string) => {
      if (action === "list")
        return Promise.resolve({
          value: {
            library_version: 4,
            published_library_version: 3,
            can_create: true,
            create_visibilities: ["private", "shared"],
            allowed_actions: ["skill_library.list", "skill_library.create"],
            items: [],
          },
          receipt: {},
        });
      if (action === "preview")
        return Promise.resolve({
          value: {
            artifact_id: "skill-x",
            revision_id: "sha256:x",
            render_mode: "inert_text",
            files: [
              { path: "SKILL.md", media_type: "text/plain", text: "<img src=x onerror=alert(1)>" },
            ],
          },
          receipt: {},
        });
      throw new Error(`unexpected ${action}`);
    });
  });

  it("offers all families and renders hostile previews as inert text", async () => {
    render(<ArtifactWorkspace profile={profile} />);
    expect(await screen.findByRole("heading", { name: "AI artifacts" })).toBeTruthy();
    for (const family of ["skills", "prompts", "agents", "hooks"])
      expect(screen.getByRole("button", { name: family })).toBeTruthy();
    fireEvent.click(await screen.findByRole("button", { name: "New skill" }));
    fireEvent.click(screen.getByRole("button", { name: "Inert preview" }));
    await waitFor(() => expect(screen.getByText("<img src=x onerror=alert(1)>")).toBeTruthy());
    expect(document.querySelector("img")).toBeNull();
    expect(artifactAction).toHaveBeenCalledWith(
      "skill",
      "preview",
      expect.any(Object),
      expect.any(AbortSignal),
    );
  });
});
