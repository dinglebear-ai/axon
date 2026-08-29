import { useState } from "react";
import { Button } from "@/components/ui/aurora/button";
import type { BackendProfile } from "@/lib/backendProfiles/model";
import { LabbyExactToolRunner } from "./LabbyExactToolRunner";
import { LabbySnippetWorkspace } from "./LabbySnippetWorkspace";
import { LoadoutWorkspace } from "./LoadoutWorkspace";

export function LabbyWorkspace({ profile }: { profile: BackendProfile }) {
  const [tab, setTab] = useState<"tools" | "snippets" | "loadouts">("loadouts");
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
      </nav>
      {tab === "loadouts" ? (
        <LoadoutWorkspace profile={profile} />
      ) : tab === "tools" ? (
        <LabbyExactToolRunner profile={profile} />
      ) : (
        <LabbySnippetWorkspace profile={profile} />
      )}
    </div>
  );
}
