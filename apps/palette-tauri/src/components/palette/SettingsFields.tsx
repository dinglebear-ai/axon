// Palette-local form field primitives extracted from SettingsPanel (finding L5).
//
// These are intentionally NOT promoted into components/ui/aurora/* — they are
// settings-panel-specific controls styled with the `settings-*` class family in
// styles.css, not part of the shared Aurora design-system layer. Keeping them
// here keeps SettingsPanel.tsx under the 500-line monolith cap while preserving
// the local-only scope.

import { Eye, EyeOff, KeyRound } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/aurora/button";
import { Input } from "@/components/ui/aurora/input";
import { NativeSelect } from "@/components/ui/aurora/native-select";

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
    <Input
      unstyled
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
      <Input
        unstyled
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
      <Button variant="plain" size="unstyled" type="button" onClick={() => setShow((visible) => !visible)} aria-label={show ? "Hide secret" : "Reveal secret"}>
        {show ? <EyeOff size={13} /> : <Eye size={13} />}
      </Button>
    </span>
  );
}

export function SelectInput({ value, options, onChange }: { value: string; options: string[]; onChange: (value: string) => void }) {
  return (
    <span className="settings-select settings-native-select">
      <NativeSelect
        value={value}
        className="settings-select-control"
        style={{ height: "34px" }}
        onChange={(event) => onChange(event.target.value)}
      >
        {options.map((option) => (
          <option key={option} value={option}>
            {option || "(unset)"}
          </option>
        ))}
      </NativeSelect>
    </span>
  );
}

export function MiniToggle({ on, onChange }: { on: boolean; onChange: (value: boolean) => void }) {
  return (
    <Button variant="plain" size="unstyled" className={on ? "settings-toggle settings-toggle-on" : "settings-toggle"} type="button" onClick={() => onChange(!on)} aria-pressed={on}>
      <span />
    </Button>
  );
}
