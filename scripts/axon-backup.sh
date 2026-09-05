#!/usr/bin/env bash
# axon-backup.sh — snapshot Qdrant collection + SQLite jobs DB
#
# Usage:
#   ./scripts/axon-backup.sh [--collection NAME] [--output-dir DIR] [--yes]
#   ./scripts/axon-backup.sh --restore SNAPSHOT [--collection NAME] [--yes]
#
# What it does:
#   1. Triggers a Qdrant snapshot via the /collections/{name}/snapshots API
#   2. Downloads the snapshot .tar.gz to OUTPUT_DIR/qdrant/
#   3. Creates a safe SQLite backup of jobs.db via the SQLite Online Backup API
#      (.backup command) into OUTPUT_DIR/sqlite/
#   4. Prints a summary with sizes and checksums
#
# Prerequisites:
#   - curl, sqlite3 on PATH
#   - QDRANT_URL env var set (or defaults to http://127.0.0.1:53333)
#   - AXON_SQLITE_PATH env var set (or defaults to ~/.axon/jobs.db)
#
# Restore uses Qdrant's snapshot-upload API so local and remote servers work.
#
# Schedule via cron (example — weekly on Sunday at 02:00):
#   0 2 * * 0 /home/user/workspace/axon/scripts/axon-backup.sh --yes >> ~/.axon/logs/backup.log 2>&1
#
# ZFS replication note:
#   If you replicate the axon host's ZFS datasets to a backup box (e.g. backuphost),
#   these backups land there automatically — no separate scp step required.

set -euo pipefail
umask 077

# ── Defaults ────────────────────────────────────────────────────────────────
QDRANT_URL="${QDRANT_URL:-http://127.0.0.1:53333}"
SQLITE_PATH="${AXON_SQLITE_PATH:-${HOME}/.axon/jobs.db}"
COLLECTION="${AXON_COLLECTION:-axon}"
OUTPUT_DIR="${AXON_BACKUP_DIR:-${HOME}/.axon/backups}"
YES=0
RESTORE_SNAPSHOT=""
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"

# ── Argument parsing ─────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --collection) COLLECTION="$2"; shift 2 ;;
        --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
        --restore) RESTORE_SNAPSHOT="$2"; shift 2 ;;
        --yes|-y)     YES=1; shift ;;
        --help|-h)
            sed -n '2,30p' "$0" | grep '^#' | sed 's/^# *//'
            exit 0 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

QDRANT_DIR="${OUTPUT_DIR}/qdrant"
SQLITE_DIR="${OUTPUT_DIR}/sqlite"
QDRANT_HEADERS=(-H "Content-Type: application/json")
if [[ -n "${QDRANT_API_KEY:-}" ]]; then
    QDRANT_HEADERS+=(-H "api-key: ${QDRANT_API_KEY}")
fi

# ── Confirmation prompt ──────────────────────────────────────────────────────
echo "axon-backup — ${TIMESTAMP}"
echo "  Qdrant:     ${QDRANT_URL}  collection=${COLLECTION}"
echo "  SQLite:     ${SQLITE_PATH}"
echo "  Output dir: ${OUTPUT_DIR}"
echo ""

if [[ "$YES" -eq 0 ]]; then
    read -rp "Proceed? [y/N] " confirm
    case "$confirm" in
        [yY]*) ;;
        *) echo "Aborted."; exit 0 ;;
    esac
fi

mkdir -p -m 0700 "${OUTPUT_DIR}" "${QDRANT_DIR}" "${SQLITE_DIR}"
chmod 0700 "${OUTPUT_DIR}" "${QDRANT_DIR}" "${SQLITE_DIR}"
output_owner="$(stat -f '%u' "$OUTPUT_DIR" 2>/dev/null || stat -c '%u' "$OUTPUT_DIR")"
[[ "$output_owner" = "$(id -u)" ]] || {
    echo "ERROR: backup output root is not owned by the invoking user: ${OUTPUT_DIR}" >&2
    exit 1
}

if [[ -n "$RESTORE_SNAPSHOT" ]]; then
    [[ -f "$RESTORE_SNAPSHOT" ]] || { echo "ERROR: snapshot not found: $RESTORE_SNAPSHOT" >&2; exit 1; }
    echo "Uploading snapshot to Qdrant for collection '${COLLECTION}'..."
    upload_headers=()
    if [[ -n "${QDRANT_API_KEY:-}" ]]; then
        upload_headers=(-H "api-key: ${QDRANT_API_KEY}")
    fi
    curl -fsS -X POST "${upload_headers[@]}" \
        -F "snapshot=@${RESTORE_SNAPSHOT}" \
        "${QDRANT_URL}/collections/${COLLECTION}/snapshots/upload?priority=snapshot"
    echo "Restore completed through the Qdrant upload API."
    exit 0
fi

# ── 1. Qdrant snapshot ───────────────────────────────────────────────────────
echo "[1/3] Creating Qdrant snapshot for collection '${COLLECTION}'..."
SNAPSHOT_RESP=$(curl -fsS -X POST \
    "${QDRANT_URL}/collections/${COLLECTION}/snapshots" \
    "${QDRANT_HEADERS[@]}")

if ! echo "$SNAPSHOT_RESP" | grep -q '"status":"ok"'; then
    echo "ERROR: Qdrant snapshot creation failed." >&2
    echo "Response: ${SNAPSHOT_RESP}" >&2
    exit 1
fi

SNAPSHOT_NAME=$(echo "$SNAPSHOT_RESP" | \
    python3 -c "import sys,json; print(json.load(sys.stdin)['result']['name'])")

echo "  Snapshot created: ${SNAPSHOT_NAME}"
echo "  Downloading..."

QDRANT_DEST="${QDRANT_DIR}/${COLLECTION}-${TIMESTAMP}.snapshot"
QDRANT_TMP="${QDRANT_DEST}.partial"
curl -fsSL \
    "${QDRANT_HEADERS[@]}" \
    "${QDRANT_URL}/collections/${COLLECTION}/snapshots/${SNAPSHOT_NAME}" \
    -o "${QDRANT_TMP}"
[[ -s "$QDRANT_TMP" ]] || { echo "ERROR: downloaded snapshot is empty" >&2; exit 1; }
chmod 0600 "$QDRANT_TMP"
mv "$QDRANT_TMP" "$QDRANT_DEST"

QDRANT_SIZE=$(du -sh "${QDRANT_DEST}" | cut -f1)
QDRANT_SHA256=$(sha256sum "${QDRANT_DEST}" | cut -d' ' -f1)
echo "  Saved: ${QDRANT_DEST} (${QDRANT_SIZE})"
echo "  SHA256: ${QDRANT_SHA256}"

# Clean up the server-side snapshot to free Qdrant storage
echo "  Deleting server-side snapshot..."
curl -fsS -X DELETE \
    "${QDRANT_HEADERS[@]}" \
    "${QDRANT_URL}/collections/${COLLECTION}/snapshots/${SNAPSHOT_NAME}" \
    > /dev/null

# ── 2. SQLite backup ─────────────────────────────────────────────────────────
echo "[2/3] Backing up SQLite jobs DB..."
SQLITE_DEST=""
if [[ ! -f "${SQLITE_PATH}" ]]; then
    echo "  WARNING: SQLite DB not found at ${SQLITE_PATH} — skipping." >&2
else
    SQLITE_DEST="${SQLITE_DIR}/jobs-${TIMESTAMP}.db"
    SQLITE_TMP="${SQLITE_DEST}.partial"
    # sqlite3 .backup is safe under concurrent writers (uses WAL/shared-cache lock)
    sqlite3 "${SQLITE_PATH}" ".backup '${SQLITE_TMP}'"
    chmod 0600 "$SQLITE_TMP"
    mv "$SQLITE_TMP" "$SQLITE_DEST"
    SQLITE_SIZE=$(du -sh "${SQLITE_DEST}" | cut -f1)
    SQLITE_SHA256=$(sha256sum "${SQLITE_DEST}" | cut -d' ' -f1)
    echo "  Saved: ${SQLITE_DEST} (${SQLITE_SIZE})"
    echo "  SHA256: ${SQLITE_SHA256}"
fi

# ── 3. Summary ───────────────────────────────────────────────────────────────
MANIFEST="${OUTPUT_DIR}/backup-${COLLECTION}-${TIMESTAMP}.json"
python3 - "$MANIFEST" "$COLLECTION" "$QDRANT_URL" "$QDRANT_DEST" "$QDRANT_SHA256" "$SQLITE_DEST" "${SQLITE_SHA256:-}" <<'PY'
import json, os, sys
dest, collection, url, snapshot, snapshot_sha, sqlite, sqlite_sha = sys.argv[1:]
with open(dest + ".partial", "w", encoding="utf-8") as fh:
    json.dump({"schema_version": 1, "collection": collection, "qdrant_url": url,
               "snapshot": snapshot, "snapshot_sha256": snapshot_sha,
               "sqlite": sqlite or None, "sqlite_sha256": sqlite_sha or None}, fh, indent=2)
    fh.write("\n")
os.chmod(dest + ".partial", 0o600)
os.replace(dest + ".partial", dest)
PY
echo ""
echo "Restore instructions:"
echo "  Qdrant: $0 --collection ${COLLECTION} --restore ${QDRANT_DEST} --yes"
if [[ -n "$SQLITE_DEST" ]]; then
    echo "  SQLite: cp ${SQLITE_DEST} ${SQLITE_PATH}   (stop workers first)"
else
    echo "  SQLite: not included (source database was absent)"
fi
echo "[3/3] Backup complete."
echo "  Manifest: ${MANIFEST}"
for protected_file in "$QDRANT_DEST" "$MANIFEST"; do
    protected_mode="$(stat -f '%Lp' "$protected_file" 2>/dev/null || stat -c '%a' "$protected_file")"
    [[ "$protected_mode" = 600 ]] || { echo "ERROR: insecure backup mode on $protected_file" >&2; exit 1; }
done
if [[ -n "$SQLITE_DEST" ]]; then
    sqlite_mode="$(stat -f '%Lp' "$SQLITE_DEST" 2>/dev/null || stat -c '%a' "$SQLITE_DEST")"
    [[ "$sqlite_mode" = 600 ]] || { echo "ERROR: insecure backup mode on $SQLITE_DEST" >&2; exit 1; }
fi
