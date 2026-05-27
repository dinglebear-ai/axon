import { SlidersHorizontal } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";
import type { PaletteConfig } from "@/lib/axonClient";

interface SettingsPanelProps {
  configError: string | null;
  draftConfig: PaletteConfig;
  shortcutOptions: readonly string[];
  onChange: (config: PaletteConfig) => void;
  onClose: () => void;
  onSave: () => void;
}

export function SettingsPanel({
  configError,
  draftConfig,
  shortcutOptions,
  onChange,
  onClose,
  onSave,
}: SettingsPanelProps) {
  const updateConfig = <Key extends keyof PaletteConfig>(key: Key, value: PaletteConfig[Key]) => {
    onChange({ ...draftConfig, [key]: value });
  };

  return (
    <section className="settings-panel">
      <div className="settings-heading">
        <SlidersHorizontal size={15} />
        <span>Settings</span>
      </div>
      <label>
        <span>Server</span>
        <input value={draftConfig.serverUrl} onChange={(event) => updateConfig("serverUrl", event.target.value)} />
      </label>
      <label>
        <span>Token</span>
        <input
          type="password"
          value={draftConfig.token ?? ""}
          onChange={(event) => updateConfig("token", event.target.value || null)}
        />
      </label>
      <label>
        <span>Shortcut</span>
        <select value={draftConfig.shortcut} onChange={(event) => updateConfig("shortcut", event.target.value)}>
          {shortcutOptions.map((shortcut) => (
            <option key={shortcut} value={shortcut}>
              {shortcut}
            </option>
          ))}
        </select>
      </label>
      <label>
        <span>Collection</span>
        <input value={draftConfig.collection} onChange={(event) => updateConfig("collection", event.target.value)} />
      </label>
      <label>
        <span>Results</span>
        <input
          type="number"
          min={1}
          max={50}
          value={draftConfig.resultLimit}
          onChange={(event) => updateConfig("resultLimit", Number(event.target.value))}
        />
      </label>
      <label>
        <span>Theme</span>
        <select value={draftConfig.theme} onChange={(event) => updateConfig("theme", event.target.value as PaletteConfig["theme"])}>
          <option value="system">System</option>
          <option value="dark">Dark</option>
          <option value="light">Light</option>
        </select>
      </label>
      <label className="settings-check">
        <input
          type="checkbox"
          checked={draftConfig.hideOnBlur}
          onChange={(event) => updateConfig("hideOnBlur", event.target.checked)}
        />
        <span>Hide on blur</span>
      </label>
      <div className="settings-actions">
        {configError && <span>{configError}</span>}
        <Button size="sm" variant="neutral" onClick={onClose}>
          Close
        </Button>
        <Button size="sm" variant="rose" onClick={onSave}>
          Save
        </Button>
      </div>
    </section>
  );
}
