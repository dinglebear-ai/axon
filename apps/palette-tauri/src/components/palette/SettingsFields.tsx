// Palette-local form field primitives extracted from SettingsPanel (finding L5).
//
// These are intentionally NOT promoted into components/ui/aurora/* — they are
// settings-panel-specific controls styled with the `settings-*` class family in
// styles.css, not part of the shared Aurora design-system layer. Keeping them
// here keeps SettingsPanel.tsx under the 500-line monolith cap while preserving
// the local-only scope.

import { ChevronDown, Eye, EyeOff, KeyRound } from "lucide-react";
import { useState } from "react";

export function TextInput({
  value,
  onChange,
  mono,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  mono?: boolean;
  placeholder?: string;
}) {
  return (
    <input
      className={mono ? "settings-input settings-input-mono" : "settings-input"}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      placeholder={placeholder}
    />
  );
}

export function SecretInput({ value, onChange, placeholder }: { value: string; onChange: (value: string) => void; placeholder?: string }) {
  const [show, setShow] = useState(false);
  return (
    <span className="settings-secret">
      <KeyRound size={12} />
      <input
        value={value}
        placeholder={placeholder ?? "unset - secret"}
        type={show ? "text" : "password"}
        onChange={(event) => onChange(event.target.value)}
        // S-I1: keep tokens/secrets out of autofill, spellcheck, and password managers.
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        data-1p-ignore
      />
      <button type="button" onClick={() => setShow((visible) => !visible)} aria-label={show ? "Hide secret" : "Reveal secret"}>
        {show ? <EyeOff size={13} /> : <Eye size={13} />}
      </button>
    </span>
  );
}

export function SelectInput({ value, options, onChange }: { value: string; options: string[]; onChange: (value: string) => void }) {
  return (
    <span className="settings-select">
      <select value={value} onChange={(event) => onChange(event.target.value)}>
        {options.map((option) => (
          <option key={option} value={option}>
            {option || "(unset)"}
          </option>
        ))}
      </select>
      <ChevronDown size={13} aria-hidden="true" />
    </span>
  );
}

export function MiniToggle({ on, onChange }: { on: boolean; onChange: (value: boolean) => void }) {
  return (
    <button className={on ? "settings-toggle settings-toggle-on" : "settings-toggle"} type="button" onClick={() => onChange(!on)} aria-pressed={on}>
      <span />
    </button>
  );
}
