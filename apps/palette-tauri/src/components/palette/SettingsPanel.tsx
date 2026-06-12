import {
  Activity,
  Brain,
  Braces,
  ChevronDown,
  Database,
  Eye,
  EyeOff,
  FileText,
  Globe,
  KeyRound,
  Layers,
  Link,
  Search,
  Server,
  Shield,
  SlidersHorizontal,
  Zap,
  type LucideIcon,
} from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/aurora/button";
import { createAxonClient, executeAction, type PaletteConfig, type PaletteResult } from "@/lib/axonClient";
import { ACTIONS } from "@/lib/actions";
import {
  CONFIG_COUNT,
  CONFIG_DEFAULTS,
  CONFIG_GROUPS,
  type ConfigField,
  ENV_COUNT,
  ENV_DEFAULTS,
  ENV_GROUPS,
} from "@/lib/configModel";
import { isRecord, strField, unwrapPayload } from "@/lib/payload";

type SettingValue = string | number | boolean | string[];
type SettingsTab = "connection" | "env" | "config";

interface SettingsPanelProps {
  configError: string | null;
  draftConfig: PaletteConfig;
  shortcutOptions: readonly string[];
  onChange: (config: PaletteConfig) => void;
  onClose: () => void;
  onSave: () => void;
}

const iconMap: Record<string, LucideIcon> = {
  activity: Activity,
  ask: Brain,
  braces: Braces,
  brain: Brain,
  database: Database,
  file: FileText,
  globe: Globe,
  key: KeyRound,
  layers: Layers,
  link: Link,
  scrape: FileText,
  search: Search,
  server: Server,
  shield: Shield,
  sliders: SlidersHorizontal,
  zap: Zap,
};

export type ConnectionStatus = "unknown" | "connected" | "error" | "checking";

export interface ConnectionTestState {
  checkedAt?: number;
  detail?: string;
  status: ConnectionStatus;
}

export function connectionFeedback(state: ConnectionTestState): { detail: string; label: string; tone: "neutral" | "success" | "error" | "checking" } {
  switch (state.status) {
    case "checking":
      return { tone: "checking", label: "Checking", detail: state.detail ?? "Testing the configured Axon server..." };
    case "connected":
      return { tone: "success", label: "Connected", detail: state.detail ?? "Doctor endpoint responded successfully." };
    case "error":
      return { tone: "error", label: "Connection failed", detail: state.detail ?? "Axon did not return a successful doctor response." };
    default:
      return { tone: "neutral", label: "Not tested", detail: "Run a connection test before saving." };
  }
}

export function SettingsPanel({
  configError,
  draftConfig,
  shortcutOptions,
  onChange,
  onClose,
  onSave,
}: SettingsPanelProps) {
  const [tab, setTab] = useState<SettingsTab>("connection");
  const [connectionTest, setConnectionTest] = useState<ConnectionTestState>({ status: "unknown" });
  const envValues = { ...ENV_DEFAULTS, ...(draftConfig.envValues ?? {}) } as Record<string, SettingValue>;
  const configValues = { ...CONFIG_DEFAULTS, ...(draftConfig.configValues ?? {}) } as Record<string, SettingValue>;
  const connectionState = connectionFeedback(connectionTest);

  const testConnection = async () => {
    setConnectionTest({ status: "checking", detail: `Testing ${draftConfig.serverUrl || "server"}...` });
    try {
      const doctorAction = ACTIONS.find((a) => a.subcommand === "doctor");
      if (!doctorAction) {
        setConnectionTest({ status: "error", checkedAt: Date.now(), detail: "Doctor action is not registered in the palette." });
        return;
      }
      const result = await executeAction(createAxonClient(draftConfig), doctorAction, "", draftConfig);
      setConnectionTest({
        status: result.ok ? "connected" : "error",
        checkedAt: Date.now(),
        detail: connectionDetailFromResult(result),
      });
    } catch (error) {
      setConnectionTest({ status: "error", checkedAt: Date.now(), detail: messageFromUnknown(error) });
    }
  };

  const updateConfig = <Key extends keyof PaletteConfig>(key: Key, value: PaletteConfig[Key]) => {
    if (key === "serverUrl" || key === "token" || key === "collection") {
      setConnectionTest({ status: "unknown" });
    }
    onChange({ ...draftConfig, [key]: value });
  };
  const updateEnv = (key: string, value: SettingValue) => {
    onChange({ ...draftConfig, envValues: { ...(draftConfig.envValues ?? {}), [key]: value } });
  };
  const updateToml = (key: string, value: SettingValue) => {
    onChange({ ...draftConfig, configValues: { ...(draftConfig.configValues ?? {}), [key]: value } });
  };

  return (
    <section className="settings-panel settings-panel-mock">
      <header className="settings-topline">
        <span className="settings-eyebrow">Settings</span>
        <span className="settings-health" data-status={connectionTest.status}>
          <span aria-hidden="true" />
          {connectionState.label.toLowerCase()}
        </span>
      </header>

      <nav className="settings-tabs" aria-label="Settings sections">
        <SettingsTabButton id="connection" label="Connection" icon="link" active={tab === "connection"} onClick={setTab} />
        <SettingsTabButton id="env" label="Environment" icon="key" count={ENV_COUNT} active={tab === "env"} onClick={setTab} />
        <SettingsTabButton id="config" label="config.toml" icon="sliders" count={CONFIG_COUNT} active={tab === "config"} onClick={setTab} />
      </nav>

      <div className="settings-scroll">
        {tab === "connection" && (
          <div className="settings-connection-grid">
            <div className="settings-stack">
              <span className="settings-section-label">Connection</span>
              <Field label="Server" hint="RMCP endpoint">
                <TextInput value={draftConfig.serverUrl} onChange={(value) => updateConfig("serverUrl", value)} mono />
              </Field>
              <Field label="Bearer token" hint="AXON_MCP_HTTP_TOKEN">
                <SecretInput value={draftConfig.token ?? ""} onChange={(value) => updateConfig("token", value || null)} />
              </Field>
              <Field label="Collection" hint="vector store">
                <TextInput value={draftConfig.collection} onChange={(value) => updateConfig("collection", value)} mono />
              </Field>
            </div>
            <div className="settings-stack">
              <span className="settings-section-label">Client</span>
              <Field label="Global shortcut" hint="press to record">
                <TextInput value={draftConfig.shortcut || shortcutOptions[0]} onChange={(value) => updateConfig("shortcut", value)} mono />
              </Field>
              <Field label="Max results">
                <TextInput
                  value={String(draftConfig.resultLimit)}
                  onChange={(value) => updateConfig("resultLimit", Number(value.replace(/\D/g, "").slice(0, 3)) || 1)}
                  mono
                />
              </Field>
              <ToggleRow
                label="Hide on blur"
                sub="Dismiss when the window loses focus"
                on={draftConfig.hideOnBlur}
                onChange={(value) => updateConfig("hideOnBlur", value)}
              />
              <ToggleRow
                label="Open results inline"
                sub="Expand the panel instead of a new window"
                on={draftConfig.openResultsInline ?? true}
                onChange={(value) => updateConfig("openResultsInline", value)}
              />
            </div>
          </div>
        )}

        {tab === "env" && (
          <div className="settings-knob-pane">
            <div className="settings-file-meta">~/.axon/.env - URLs, secrets, auth, runtime bootstrap</div>
            {ENV_GROUPS.map((group) => (
              <KnobGroup
                key={group.id}
                icon={group.icon}
                title={group.label}
                count={`${group.vars.length} vars`}
                note={group.note}
                fields={group.vars}
                mono
                values={envValues}
                nameOf={(field) => field.key}
                onChange={updateEnv}
              />
            ))}
          </div>
        )}

        {tab === "config" && (
          <div className="settings-knob-pane">
            <div className="settings-file-meta">~/.axon/config.toml - non-secret tuning; env var overrides each value</div>
            {CONFIG_GROUPS.map((group) => (
              <KnobGroup
                key={group.id}
                icon={group.icon}
                title={group.label}
                badge={group.section}
                count={`${group.knobs.length} knobs`}
                note={group.note}
                fields={group.knobs}
                values={configValues}
                nameOf={(field) => `${sectionName(group.section)}.${field.key}`}
                onChange={updateToml}
              />
            ))}
          </div>
        )}
      </div>

      <footer className="settings-footer">
        <Button
          size="sm"
          variant="neutral"
          onClick={() => void testConnection()}
          disabled={connectionTest.status === "checking"}
          aria-label="Test Axon server connection"
        >
          <Activity size={14} />
          {connectionTest.status === "checking" ? "Checking…" : "Test connection"}
        </Button>
        {connectionTest.status === "unknown" ? (
          <span className="settings-footer-meta">
            {tab === "env" ? `${ENV_COUNT} env vars` : tab === "config" ? `${CONFIG_COUNT} config knobs` : "precedence: CLI > env > config.toml > defaults"}
          </span>
        ) : (
          <span className="settings-connection-result" data-status={connectionState.tone} aria-live="polite">
            <span aria-hidden="true" />
            <span>
              <strong>{connectionState.label}</strong>
              <span>{connectionState.detail}</span>
            </span>
          </span>
        )}
        {configError && <span className="settings-error">{configError}</span>}
        <div className="settings-footer-actions">
          <Button size="sm" variant="neutral" onClick={onClose}>
            Close
          </Button>
          <Button size="sm" variant="aurora" onClick={onSave}>
            Save
          </Button>
        </div>
      </footer>
    </section>
  );
}

function connectionDetailFromResult(result: PaletteResult): string {
  const payload = unwrapPayload(result.payload);
  const detail = isRecord(payload) ? (strField(payload, "message") ?? strField(payload, "error") ?? strField(payload, "detail")) : undefined;
  if (detail) return detail;
  if (result.ok) return `${result.method} ${result.path} responded with HTTP ${result.status}.`;
  return `HTTP ${result.status || "local"} from ${result.method} ${result.path}.`;
}

function messageFromUnknown(error: unknown): string {
  return error instanceof Error ? error.message : "Connection test failed before Axon returned a response.";
}

function sectionName(section: string): string {
  return section.replace(/^\[/, "").replace(/\]$/, "");
}

function SettingsTabButton({
  id,
  label,
  icon,
  count,
  active,
  onClick,
}: {
  id: SettingsTab;
  label: string;
  icon: string;
  count?: number;
  active: boolean;
  onClick: (id: SettingsTab) => void;
}) {
  const Icon = iconMap[icon] ?? SlidersHorizontal;
  return (
    <button className={active ? "settings-tab settings-tab-active" : "settings-tab"} type="button" onClick={() => onClick(id)}>
      <Icon size={13} />
      {label}
      {count != null && <span>{count}</span>}
    </button>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="settings-field">
      <span className="settings-field-head">
        <span>{label}</span>
        {hint && <span>{hint}</span>}
      </span>
      {children}
    </label>
  );
}

function TextInput({
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

function SecretInput({ value, onChange, placeholder }: { value: string; onChange: (value: string) => void; placeholder?: string }) {
  const [show, setShow] = useState(false);
  return (
    <span className="settings-secret">
      <KeyRound size={12} />
      <input
        value={value}
        placeholder={placeholder ?? "unset - secret"}
        type={show ? "text" : "password"}
        onChange={(event) => onChange(event.target.value)}
      />
      <button type="button" onClick={() => setShow((visible) => !visible)} aria-label={show ? "Hide secret" : "Reveal secret"}>
        {show ? <EyeOff size={13} /> : <Eye size={13} />}
      </button>
    </span>
  );
}

function SelectInput({ value, options, onChange }: { value: string; options: string[]; onChange: (value: string) => void }) {
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

function MiniToggle({ on, onChange }: { on: boolean; onChange: (value: boolean) => void }) {
  return (
    <button className={on ? "settings-toggle settings-toggle-on" : "settings-toggle"} type="button" onClick={() => onChange(!on)} aria-pressed={on}>
      <span />
    </button>
  );
}

function ToggleRow({ label, sub, on, onChange }: { label: string; sub?: string; on: boolean; onChange: (value: boolean) => void }) {
  return (
    <div className="settings-toggle-row">
      <span>
        <span>{label}</span>
        {sub && <span>{sub}</span>}
      </span>
      <MiniToggle on={on} onChange={onChange} />
    </div>
  );
}

function KnobGroup({
  icon,
  title,
  badge,
  count,
  note,
  fields,
  mono,
  values,
  nameOf,
  onChange,
}: {
  icon: string;
  title: string;
  badge?: string;
  count: string;
  note: string;
  fields: ConfigField[];
  mono?: boolean;
  values: Record<string, SettingValue>;
  nameOf: (field: ConfigField) => string;
  onChange: (key: string, value: SettingValue) => void;
}) {
  const Icon = iconMap[icon] ?? SlidersHorizontal;
  return (
    <section className="settings-knob-group">
      <header className="settings-knob-head">
        <span className="settings-knob-icon">
          <Icon size={14} />
        </span>
        {badge && <code>{badge}</code>}
        <span>{title}</span>
        <span>{count}</span>
      </header>
      <p>{note}</p>
      <div className="settings-knob-grid">
        {fields.map((field) => {
          const key = nameOf(field);
          return <KnobRow key={key} field={field} name={field.key} mono={mono} value={values[key] ?? field.def} onChange={(value) => onChange(key, value)} />;
        })}
      </div>
    </section>
  );
}

function KnobRow({
  field,
  name,
  mono,
  value,
  onChange,
}: {
  field: ConfigField;
  name: string;
  mono?: boolean;
  value: SettingValue;
  onChange: (value: SettingValue) => void;
}) {
  const isBool = field.type === "bool";
  const placeholder = field.def === "" || field.def == null ? "unset" : String(field.def);
  return (
    <div className="settings-knob-row">
      <div className="settings-knob-title">
        <span className={mono ? "settings-mono-label" : undefined}>{name}</span>
        {field.env && <span title={`env: ${field.env}`}>env</span>}
        {isBool && <MiniToggle on={Boolean(value)} onChange={onChange} />}
      </div>
      {!isBool && (
        <>
          {field.type === "enum" ? (
            <SelectInput value={String(value ?? "")} options={field.options ?? []} onChange={onChange} />
          ) : field.type === "secret" ? (
            <SecretInput value={String(value ?? "")} onChange={onChange} />
          ) : (
            <TextInput
              value={field.type === "list" && Array.isArray(value) ? value.join(", ") : String(value ?? "")}
              onChange={(next) => onChange(field.type === "list" ? next.split(",").map((item) => item.trim()).filter(Boolean) : next)}
              mono
              placeholder={field.type === "list" ? "comma,separated" : placeholder}
            />
          )}
        </>
      )}
      <span className="settings-knob-desc">{field.desc}</span>
    </div>
  );
}
