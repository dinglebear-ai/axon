import { Button } from "@/components/ui/aurora/button";
import type { BackendProduct, BackendProfile } from "@/lib/backendProfiles/model";
import { NativeSelect } from "@/components/ui/aurora/native-select";

const PRODUCTS: Array<{ id: BackendProduct; label: string; purpose: string }> = [
  { id: "axon", label: "Axon", purpose: "Knowledge & work" },
  { id: "labby", label: "Labby", purpose: "Gateway & capabilities" },
  { id: "cortex", label: "Cortex", purpose: "Observability" },
];

export function ProductNavigation({
  active,
  available,
  onSelect,
  profiles = [],
  activeProfileIds = {},
  onSelectProfile,
}: {
  active: BackendProduct;
  available: ReadonlySet<BackendProduct>;
  onSelect: (product: BackendProduct) => void;
  profiles?: BackendProfile[];
  activeProfileIds?: Partial<Record<BackendProduct, string>>;
  onSelectProfile?: (product: BackendProduct, profileId: string) => void;
}) {
  return (
    <nav className="product-navigation" aria-label="Product workspaces">
      {PRODUCTS.map((product) => {
        const configured = available.has(product.id);
        const productProfiles = profiles.filter((profile) => profile.product === product.id);
        const selectedProfile = productProfiles.find(
          (profile) => profile.id === activeProfileIds[product.id],
        );
        return (
          <div key={product.id} className="product-navigation-choice">
            <Button
              variant={active === product.id ? "aurora" : "plain"}
              className="product-navigation-item"
              type="button"
              aria-current={active === product.id ? "page" : undefined}
              aria-describedby={`product-${product.id}-purpose`}
              onClick={() => onSelect(product.id)}
            >
              <span>{product.label}</span>
              <span id={`product-${product.id}-purpose`} className="product-navigation-purpose">
                {product.purpose}
                {configured ? "" : " · setup required"}
              </span>
            </Button>
            {active === product.id && productProfiles.length > 1 ? (
              <NativeSelect
                aria-label={`${product.label} active profile`}
                value={activeProfileIds[product.id] ?? ""}
                onChange={(event) => onSelectProfile?.(product.id, event.target.value)}
              >
                <option value="" disabled>
                  Select profile
                </option>
                {productProfiles.map((profile) => (
                  <option key={profile.id} value={profile.id}>
                    {profile.label} · {profile.origin}
                  </option>
                ))}
              </NativeSelect>
            ) : active === product.id ? (
              <span className="product-active-profile">
                {selectedProfile
                  ? `${selectedProfile.label} · ${selectedProfile.origin}`
                  : "No active profile selected"}
              </span>
            ) : null}
          </div>
        );
      })}
    </nav>
  );
}
