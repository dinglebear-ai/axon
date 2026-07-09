import { Button } from "@/components/ui/aurora/button";
import type { SftpKnownHostEntry } from "@/lib/sftpModel";

export function SftpTrustPrompt({
  entry,
  onTrust,
  onCancel,
}: {
  entry: SftpKnownHostEntry;
  onTrust: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="sftp-trust-prompt" role="alertdialog" aria-label="Confirm SFTP host key">
      <p>
        First connection to{" "}
        <strong>
          {entry.host}:{entry.port}
        </strong>
        . This host's key will be remembered — future connections will fail if the key ever changes
        unexpectedly.
      </p>
      <p className="sftp-trust-fingerprint">
        {entry.keyType} · {entry.fingerprint}
      </p>
      <div className="sftp-trust-actions">
        <Button variant="ghost" size="sm" type="button" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant="aurora" size="sm" type="button" onClick={onTrust}>
          Trust and connect
        </Button>
      </div>
    </div>
  );
}
