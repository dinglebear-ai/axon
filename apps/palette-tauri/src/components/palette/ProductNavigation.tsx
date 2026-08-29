import { Button } from "@/components/ui/aurora/button";
import type { BackendProduct } from "@/lib/backendProfiles/model";

const PRODUCTS: Array<{ id: BackendProduct; label: string; purpose: string }> = [
  { id: "axon", label: "Axon", purpose: "Knowledge & work" },
  { id: "labby", label: "Labby", purpose: "Gateway & capabilities" },
  { id: "cortex", label: "Cortex", purpose: "Observability" },
];

export function ProductNavigation({
  active,
  available,
  onSelect,
}: {
  active: BackendProduct;
  available: ReadonlySet<BackendProduct>;
  onSelect: (product: BackendProduct) => void;
}) {
  return (
    <nav className="product-navigation" aria-label="Product workspaces">
      {PRODUCTS.map((product) => {
        const configured = available.has(product.id);
        return (
          <Button
            key={product.id}
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
        );
      })}
    </nav>
  );
}
