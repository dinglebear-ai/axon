import { Activity, ArrowLeft } from "lucide-react";
import { useState } from "react";

import { SettingsAuthBlock } from "@/components/palette/SettingsAuthBlock";
import { MiniToggle, SecretInput, TextInput } from "@/components/palette/SettingsFields";
import { Button } from "@/components/ui/aurora/button";
import { ACTIONS } from "@/lib/actions";
import {
  createAxonClient,
  executeAction,
  type PaletteConfig,
  type PaletteResult,
} from "@/lib/axonClient";
import { isRecord, strField, unwrapPayload } from "@/lib/payload";
import { invoke } from "@/lib/invoke";
import type { BackendProfile, BackendProduct } from "@/lib/backendProfiles/model";

interface SettingsPanelProps {
  configError: string | null;
  draftConfig: PaletteConfig;
  mobile?: boolean;
  shortcutOptions: readonly string[];
  onChange: (config: PaletteConfig) => void;
  onClose: () => void;
  onSave: () => void;
}

export type ConnectionStatus = "unknown" | "connected" | "error" | "checking";

export interface ConnectionTestState {
  checkedAt?: number;
  detail?: string;
  status: ConnectionStatus;
}

export function connectionFeedback(state: ConnectionTestState): {
  detail: string;
  label: string;
  tone: "neutral" | "success" | "error" | "checking";
} {
  switch (state.status) {
    case "checking":
      return {
        tone: "checking",
        label: "Checking",
        detail: state.detail ?? "Testing the configured Axon server...",
      };
    case "connected":
      return {
        tone: "success",
        label: "Connected",
        detail: state.detail ?? "Doctor endpoint responded successfully.",
      };
    case "error":
      return {
        tone: "error",
        label: "Connection failed",
        detail: state.detail ?? "Axon did not return a successful doctor response.",
      };
    default:
      return {
        tone: "neutral",
        label: "Not tested",
        detail: "Run a connection test before saving.",
      };
  }
}

export function SettingsPanel({
  configError,
  draftConfig,
  mobile = false,
  shortcutOptions,
  onChange,
  onClose,
  onSave,
}: SettingsPanelProps) {
  const [connectionTest, setConnectionTest] = useState<ConnectionTestState>({ status: "unknown" });
  const connectionState = connectionFeedback(connectionTest);

  const testConnection = async () => {
    setConnectionTest({
      status: "checking",
      detail: `Testing ${draftConfig.serverUrl || "server"}...`,
    });
    try {
      const doctorAction = ACTIONS.find((a) => a.subcommand === "doctor");
      if (!doctorAction || doctorAction.kind === "local") {
        setConnectionTest({
          status: "error",
          checkedAt: Date.now(),
          detail: "Doctor action is not registered in the palette.",
        });
        return;
      }
      const result = await executeAction(
        createAxonClient(draftConfig),
        doctorAction,
        "",
        draftConfig,
      );
      setConnectionTest({
        status: result.ok ? "connected" : "error",
        checkedAt: Date.now(),
        detail: connectionDetailFromResult(result),
      });
    } catch (error) {
      setConnectionTest({
        status: "error",
        checkedAt: Date.now(),
        detail: messageFromUnknown(error),
      });
    }
  };

  const updateConfig = <Key extends keyof PaletteConfig>(key: Key, value: PaletteConfig[Key]) => {
    if (key === "serverUrl" || key === "token") {
      setConnectionTest({ status: "unknown" });
    }
    onChange({ ...draftConfig, [key]: value });
  };

  return (
    <section className="settings-panel settings-panel-mock">
      <header className="settings-topline">
        <div className="settings-heading">
          {mobile && (
            <Button
              variant="plain"
              size="unstyled"
              className="settings-mobile-back"
              type="button"
              onClick={onClose}
              aria-label="Back"
            >
              <ArrowLeft size={20} strokeWidth={1.9} />
            </Button>
          )}
          <span className="settings-eyebrow">Settings</span>
        </div>
        <span className="settings-health" data-status={connectionTest.status}>
          <span aria-hidden="true" />
          {connectionState.label.toLowerCase()}
        </span>
      </header>

      <div className="settings-scroll">
        <ConnectionPanel
          draftConfig={draftConfig}
          mobile={mobile}
          shortcutOptions={shortcutOptions}
          updateConfig={updateConfig}
        />
        <BackendProfilesSettings draftConfig={draftConfig} onChange={onChange} />
      </div>

      <SettingsFooter
        configError={configError}
        connectionTest={connectionTest}
        connectionState={connectionState}
        onTest={() => void testConnection()}
        onClose={onClose}
        onSave={onSave}
      />
    </section>
  );
}

export function BackendProfilesSettings({
  draftConfig,
  onChange,
}: {
  draftConfig: PaletteConfig;
  onChange: (config: PaletteConfig) => void;
}) {
  const [tokens, setTokens] = useState<Record<string, string>>({});
  const profiles = draftConfig.backendProfiles ?? [];
  const updateProfile = (id: string, patch: Partial<BackendProfile>) =>
    onChange({
      ...draftConfig,
      backendProfiles: profiles.map((profile) =>
        profile.id === id ? { ...profile, ...patch } : profile,
      ),
    });
  const saveCredential = async (profile: BackendProfile) => {
    const token = tokens[profile.id]?.trim() ?? "";
    if (!token) throw new Error("Enter a credential before saving or rotating it.");
    const handle = profile.credentialHandle ?? `${profile.product}:${profile.id}`;
    const generation = String(Date.now());
    await invoke("save_backend_credential", {
      credential: {
        handle,
        profileId: profile.id,
        product: profile.product,
        origin: profile.origin,
        serverId: profile.pinnedServerId ?? "unverified",
        generation,
        token,
      },
    });
    updateProfile(profile.id, { credentialHandle: handle, credentialGeneration: generation });
    setTokens((current) => ({ ...current, [profile.id]: "" }));
  };
  const removeCredential = async (profile: BackendProfile) => {
    await invoke("delete_backend_credential", { profileId: profile.id });
    updateProfile(profile.id, { credentialHandle: null, credentialGeneration: null });
    setTokens((current) => ({ ...current, [profile.id]: "" }));
  };
  const deleteProfile = async (profile: BackendProfile) => {
    await invoke("delete_backend_credential", { profileId: profile.id });
    const active = { ...draftConfig.activeBackendProfiles };
    if (active[profile.product] === profile.id) delete active[profile.product];
    onChange({
      ...draftConfig,
      backendProfiles: profiles.filter((value) => value.id !== profile.id),
      activeBackendProfiles: active,
    });
  };
  const addProfile = (product: BackendProduct) => {
    const id = `${product}-${crypto.randomUUID()}`;
    onChange({
      ...draftConfig,
      backendProfiles: [
        ...profiles,
        {
          id,
          label: `${product[0].toUpperCase()}${product.slice(1)}`,
          product,
          origin: "https://",
          credentialHandle: null,
          pinnedServerId: null,
          acceptedApiMajor: 1,
        },
      ],
    });
  };
  return (
    <div className="settings-stack" aria-label="Backend profiles">
      <span className="settings-section-label">Product backends</span>
      {profiles.map((profile) => (
        <div className="settings-stack" key={profile.id} data-testid={`backend-profile-${profile.id}`}>
          <Field label={`${profile.product} profile label`}>
            <TextInput value={profile.label} onChange={(label) => updateProfile(profile.id, { label })} />
          </Field>
          <Field label="Origin">
            <TextInput value={profile.origin} onChange={(origin) => updateProfile(profile.id, { origin })} mono />
          </Field>
          <Field label="Credential" hint={profile.credentialHandle ? "saved" : "not saved"}>
            <SecretInput value={tokens[profile.id] ?? ""} onChange={(token) => setTokens((current) => ({ ...current, [profile.id]: token }))} />
          </Field>
          <div className="settings-footer-actions">
            <Button size="sm" variant="neutral" onClick={() => void saveCredential(profile)}>
              {profile.credentialHandle ? "Rotate credential" : "Save credential"}
            </Button>
            <Button size="sm" variant="neutral" disabled={!profile.credentialHandle} onClick={() => void removeCredential(profile)}>Remove credential</Button>
            <Button size="sm" variant="neutral" onClick={() => void deleteProfile(profile)}>Delete profile</Button>
          </div>
        </div>
      ))}
      <div className="settings-footer-actions">
        {(["axon", "labby", "cortex"] as const).map((product) => (
          <Button key={product} size="sm" variant="neutral" onClick={() => addProfile(product)}>Add {product}</Button>
        ))}
      </div>
    </div>
  );
}

function ConnectionPanel({
  draftConfig,
  mobile,
  shortcutOptions,
  updateConfig,
}: {
  draftConfig: PaletteConfig;
  mobile: boolean;
  shortcutOptions: readonly string[];
  updateConfig: <Key extends keyof PaletteConfig>(key: Key, value: PaletteConfig[Key]) => void;
}) {
  return (
    <div className="settings-connection-grid">
      <div className="settings-stack">
        <span className="settings-section-label">Connection</span>
        <Field label="Server URL" hint="Axon endpoint">
          <TextInput
            value={draftConfig.serverUrl}
            onChange={(value) => updateConfig("serverUrl", value)}
            mono
          />
        </Field>
        <Field label="Bearer token" hint="optional with OAuth">
          <SecretInput
            value={draftConfig.token ?? ""}
            onChange={(value) => updateConfig("token", value || null)}
          />
        </Field>
      </div>
      <SettingsAuthBlock />
      <div className="settings-stack">
        <span className="settings-section-label">Client</span>
        {!mobile && (
          <Field label="Global shortcut" hint="press to record">
            <TextInput
              value={draftConfig.shortcut || shortcutOptions[0]}
              onChange={(value) => updateConfig("shortcut", value)}
              mono
            />
          </Field>
        )}
        <Field label="Max results">
          <TextInput
            value={String(draftConfig.resultLimit)}
            onChange={(value) =>
              updateConfig("resultLimit", Number(value.replace(/\D/g, "").slice(0, 3)) || 1)
            }
            mono
          />
        </Field>
        {!mobile && (
          <ToggleRow
            label="Hide on blur"
            sub="Dismiss when the window loses focus"
            on={draftConfig.hideOnBlur}
            onChange={(value) => updateConfig("hideOnBlur", value)}
          />
        )}
        <ToggleRow
          label="Open results inline"
          sub="Expand the panel instead of a new window"
          on={draftConfig.openResultsInline ?? true}
          onChange={(value) => updateConfig("openResultsInline", value)}
        />
        <ToggleRow
          label="Agent replies in a bubble"
          sub="Frame assistant messages like user messages"
          on={draftConfig.agentBubbles ?? false}
          onChange={(value) => updateConfig("agentBubbles", value)}
        />
        <ToggleRow
          label="Show footer hints"
          sub="Display the keyboard hint legend under the palette"
          on={draftConfig.showFooterHints ?? false}
          onChange={(value) => updateConfig("showFooterHints", value)}
        />
      </div>
    </div>
  );
}

function SettingsFooter({
  configError,
  connectionTest,
  connectionState,
  onTest,
  onClose,
  onSave,
}: {
  configError: string | null;
  connectionTest: ConnectionTestState;
  connectionState: ReturnType<typeof connectionFeedback>;
  onTest: () => void;
  onClose: () => void;
  onSave: () => void;
}) {
  return (
    <footer className="settings-footer">
      <Button
        size="sm"
        variant="neutral"
        onClick={onTest}
        disabled={connectionTest.status === "checking"}
        aria-label="Test Axon server connection"
      >
        <Activity size={14} />
        {connectionTest.status === "checking" ? "Checking…" : "Test connection"}
      </Button>
      {connectionTest.status === "unknown" ? (
        <span className="settings-footer-meta">
          Connect with a bearer token or sign in with OAuth
        </span>
      ) : (
        <span
          className="settings-connection-result"
          data-status={connectionState.tone}
          aria-live="polite"
        >
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
  );
}

function connectionDetailFromResult(result: PaletteResult): string {
  const payload = unwrapPayload(result.payload);
  const detail = isRecord(payload)
    ? (strField(payload, "message") ?? strField(payload, "error") ?? strField(payload, "detail"))
    : undefined;
  if (detail) return detail;
  if (result.ok) return `${result.method} ${result.path} responded with HTTP ${result.status}.`;
  return `HTTP ${result.status || "local"} from ${result.method} ${result.path}.`;
}

function messageFromUnknown(error: unknown): string {
  return error instanceof Error
    ? error.message
    : "Connection test failed before Axon returned a response.";
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    // biome-ignore lint/a11y/noLabelWithoutControl: the form control is passed as `children` and rendered inside this wrapping label (implicit association)
    <label className="settings-field">
      <span className="settings-field-head">
        <span>{label}</span>
        {hint && <span>{hint}</span>}
      </span>
      {children}
    </label>
  );
}

function ToggleRow({
  label,
  sub,
  on,
  onChange,
}: {
  label: string;
  sub?: string;
  on: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="settings-toggle-row">
      <span>
        <span>{label}</span>
        {sub && <span>{sub}</span>}
      </span>
      <MiniToggle label={label} on={on} onChange={onChange} />
    </div>
  );
}
