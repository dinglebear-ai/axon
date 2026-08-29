import type { ReactNode } from "react";
import { CortexWorkspace } from "./cortex/CortexWorkspace";
import { LabbyWorkspace } from "./labby/LabbyWorkspace";
import { ProductNavigation } from "./ProductNavigation";
import type { BackendProduct, BackendProfile } from "@/lib/backendProfiles/model";

export function ProductWorkspaceFrame({
  workspace,
  profiles,
  activeProfileIds,
  available,
  labbyProfile,
  cortexProfile,
  onSelect,
  onSelectProfile,
  children,
}: {
  workspace: BackendProduct;
  profiles: BackendProfile[];
  activeProfileIds: Partial<Record<BackendProduct, string>>;
  available: ReadonlySet<BackendProduct>;
  labbyProfile: BackendProfile | null;
  cortexProfile: BackendProfile | null;
  onSelect: (product: BackendProduct) => void;
  onSelectProfile: (product: BackendProduct, id: string) => void;
  children: ReactNode;
}) {
  let content = children;
  if (workspace === "labby")
    content = labbyProfile ? (
      <LabbyWorkspace key={labbyProfile.id} profile={labbyProfile} />
    ) : (
      <MissingProductProfile product="Labby" />
    );
  if (workspace === "cortex")
    content = cortexProfile ? (
      <CortexWorkspace key={cortexProfile.id} profile={cortexProfile} />
    ) : (
      <MissingProductProfile product="Cortex" />
    );
  return (
    <div className="product-workspace-shell">
      <ProductNavigation
        active={workspace}
        available={available}
        onSelect={onSelect}
        profiles={profiles}
        activeProfileIds={activeProfileIds}
        onSelectProfile={onSelectProfile}
      />
      {content}
    </div>
  );
}

function MissingProductProfile({ product }: { product: "Labby" | "Cortex" }) {
  return (
    <main className="missing-product-profile" aria-labelledby="missing-product-profile-title">
      <h1 id="missing-product-profile-title">{product} needs a backend profile</h1>
      <p>Open Settings to add its independent endpoint, credential, and trusted server identity.</p>
    </main>
  );
}
