import { useEffect, useState } from "react";

import { Button } from "@/components/ui/aurora/button";
import { invoke, isTauriRuntime } from "@/lib/invoke";
import { isValidConnectionDraft, type SftpConnectionDraft, type SftpKnownHostEntry } from "@/lib/sftpModel";

export function SftpConnectionDialog({
  draft,
  onChange,
  onSubmit,
  onClose,
}: {
  draft: SftpConnectionDraft;
  onChange: (draft: SftpConnectionDraft) => void;
  onSubmit: (draft: SftpConnectionDraft) => void;
  onClose: () => void;
}) {
  const valid = isValidConnectionDraft(draft);
  // Known-hosts management: `sftp_list_known_hosts` and `sftp_revoke_known_host`
  // were registered commands with zero frontend callers (P2 #12) — a user
  // could never review or revoke a pinned host fingerprint short of manually
  // editing sftp_known_hosts.json. This is a minimal list-and-revoke section,
  // not a full standalone UI, surfaced here since the connection dialog is
  // already the natural place a user thinks about "which host am I trusting."
  const [knownHosts, setKnownHosts] = useState<SftpKnownHostEntry[]>([]);

  useEffect(() => {
    if (!isTauriRuntime) return;
    invoke<SftpKnownHostEntry[]>("sftp_list_known_hosts")
      .then(setKnownHosts)
      .catch(() => setKnownHosts([]));
  }, []);

  function forgetHost(entry: SftpKnownHostEntry) {
    void invoke("sftp_revoke_known_host", { host: entry.host, port: entry.port })
      .then(() => setKnownHosts((prev) => prev.filter((h) => !(h.host === entry.host && h.port === entry.port))))
      .catch(() => {});
  }

  return (
    <div className="sftp-connection-dialog" role="dialog" aria-label="Add SFTP connection">
      <label>
        Label
        <input value={draft.label} onChange={(e) => onChange({ ...draft, label: e.target.value })} />
      </label>
      <label>
        Host
        <input value={draft.host} onChange={(e) => onChange({ ...draft, host: e.target.value })} />
      </label>
      <label>
        Port
        <input
          type="number"
          value={draft.port}
          onChange={(e) => onChange({ ...draft, port: Number(e.target.value) })}
        />
      </label>
      <label>
        Username
        <input value={draft.username} onChange={(e) => onChange({ ...draft, username: e.target.value })} />
      </label>
      <label>
        Private key path
        <input
          value={draft.privateKeyPath}
          onChange={(e) => onChange({ ...draft, privateKeyPath: e.target.value })}
        />
      </label>
      <div className="sftp-connection-dialog-actions">
        <Button variant="ghost" size="sm" type="button" onClick={onClose}>
          Cancel
        </Button>
        <Button variant="aurora" size="sm" type="button" disabled={!valid} onClick={() => onSubmit(draft)}>
          Connect
        </Button>
      </div>
      {knownHosts.length > 0 && (
        <div className="sftp-known-hosts">
          <p className="sftp-known-hosts-heading">Trusted host keys</p>
          <ul>
            {knownHosts.map((entry) => (
              <li key={`${entry.host}:${entry.port}`} className="sftp-known-hosts-entry">
                <span>
                  {entry.host}:{entry.port} · {entry.keyType}
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  type="button"
                  onClick={() => forgetHost(entry)}
                  aria-label={`Forget host key for ${entry.host}:${entry.port}`}
                >
                  Forget
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
