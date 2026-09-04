import type { LucideIcon } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

export interface FilesContextMenuItem {
  label: string;
  icon: LucideIcon;
  onSelect: () => void;
  disabled?: boolean;
  separatorBefore?: boolean;
}

export function FilesContextMenu({
  x,
  y,
  label,
  items,
  onClose,
}: {
  x: number;
  y: number;
  label: string;
  items: FilesContextMenuItem[];
  onClose: () => void;
}) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ x, y });

  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (!menu) return;
    const bounds = menu.getBoundingClientRect();
    setPosition({
      x: Math.max(8, Math.min(x, window.innerWidth - bounds.width - 8)),
      y: Math.max(8, Math.min(y, window.innerHeight - bounds.height - 8)),
    });
    menu.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [x, y]);

  useEffect(() => {
    function dismiss(event: PointerEvent) {
      if (!menuRef.current?.contains(event.target as Node)) onClose();
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    window.addEventListener("pointerdown", dismiss);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("pointerdown", dismiss);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  return createPortal(
    <div
      ref={menuRef}
      className="files-context-menu"
      style={{ left: position.x, top: position.y }}
      role="menu"
      aria-label={label}
      onContextMenu={(event) => event.preventDefault()}
    >
      {items.map(({ label: itemLabel, icon: Icon, onSelect, disabled, separatorBefore }) => (
        <button
          key={itemLabel}
          type="button"
          role="menuitem"
          className={separatorBefore ? "has-separator" : undefined}
          disabled={disabled}
          onClick={() => {
            onSelect();
            onClose();
          }}
        >
          <Icon size={14} strokeWidth={1.8} />
          <span>{itemLabel}</span>
        </button>
      ))}
    </div>,
    document.body,
  );
}
