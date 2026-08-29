import { useState } from "react";
import { Button } from "@/components/ui/aurora/button";
import type { BackendProfile } from "@/lib/backendProfiles/model";
import { LabbyExactToolRunner } from "./LabbyExactToolRunner";
import { LabbySnippetWorkspace } from "./LabbySnippetWorkspace";
import { LoadoutWorkspace } from "./LoadoutWorkspace";
import { ArtifactWorkspace } from "./ArtifactWorkspace";

export function LabbyWorkspace({ profile }: { profile: BackendProfile }) {
  const [tab, setTab] = useState<"tools" | "snippets" | "loadouts" | "artifacts">("loadouts");
  return (
    <div className="labby-workspace">
      <nav className="labby-workspace-tabs" aria-label="Labby workspace">
        <Button variant={tab === "tools" ? "aurora" : "plain"} onClick={() => setTab("tools")}>
          Exact tools
        </Button>
        <Button
          variant={tab === "snippets" ? "aurora" : "plain"}
          onClick={() => setTab("snippets")}
        >
          Snippets
        </Button>
        <Button
          variant={tab === "loadouts" ? "aurora" : "plain"}
          onClick={() => setTab("loadouts")}
        >
          Loadouts
        </Button>
        <Button
          variant={tab === "artifacts" ? "aurora" : "plain"}
          onClick={() => setTab("artifacts")}
        >
          Artifacts
        </Button>
      </nav>
      {tab === "artifacts" ? (
        <ArtifactWorkspace profile={profile} />
      ) : tab === "loadouts" ? (
        <LoadoutWorkspace profile={profile} />
      ) : tab === "tools" ? (
        <LabbyExactToolRunner profile={profile} />
      ) : (
        <LabbySnippetWorkspace profile={profile} />
      )}
    </div>
  );
}
