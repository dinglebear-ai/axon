#!/usr/bin/env bash
# axon-backup.sh — snapshot Qdrant collection + SQLite jobs DB
#
# Usage:
#   ./scripts/axon-backup.sh [--collection NAME] [--output-dir DIR] [--yes]
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
# Restore:
#   Qdrant: POST /collections/{name}/snapshots/recover  { "location": "file:///..." }
#   SQLite: cp backup.db jobs.db   (stop workers first)
#
# Schedule via cron (example — weekly on Sunday at 02:00):
#   0 2 * * 0 /home/user/workspace/axon/scripts/axon-backup.sh --yes >> ~/.axon/logs/backup.log 2>&1
#
# ZFS replication note:
#   If you replicate the axon host's ZFS datasets to a backup box (e.g. shart),
#   these backups land there automatically — no separate scp step required.

set -euo pipefail

# ── Defaults ────────────────────────────────────────────────────────────────
QDRANT_URL="${QDRANT_URL:-http://127.0.0.1:53333}"
SQLITE_PATH="${AXON_SQLITE_PATH:-${HOME}/.axon/jobs.db}"
COLLECTION="${AXON_COLLECTION:-axon}"
OUTPUT_DIR="${AXON_BACKUP_DIR:-${HOME}/.axon/backups}"
YES=0
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"

# ── Argument parsing ─────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --collection) COLLECTION="$2"; shift 2 ;;
        --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
        --yes|-y)     YES=1; shift ;;
        --help|-h)
            sed -n '2,30p' "$0" | grep '^#' | sed 's/^# *//'
            exit 0 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

QDRANT_DIR="${OUTPUT_DIR}/qdrant"
SQLITE_DIR="${OUTPUT_DIR}/sqlite"

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

mkdir -p "${QDRANT_DIR}" "${SQLITE_DIR}"

# ── 1. Qdrant snapshot ───────────────────────────────────────────────────────
echo "[1/3] Creating Qdrant snapshot for collection '${COLLECTION}'..."
SNAPSHOT_RESP=$(curl -fsS -X POST \
    "${QDRANT_URL}/collections/${COLLECTION}/snapshots" \
    -H "Content-Type: application/json")

if ! echo "$SNAPSHOT_RESP" | grep -q '"status":"ok"'; then
    echo "ERROR: Qdrant snapshot creation failed." >&2
    echo "Response: ${SNAPSHOT_RESP}" >&2
    exit 1
fi

SNAPSHOT_NAME=$(echo "$SNAPSHOT_RESP" | \
    python3 -c "import sys,json; print(json.load(sys.stdin)['result']['name'])")

echo "  Snapshot created: ${SNAPSHOT_NAME}"
echo "  Downloading..."

QDRANT_DEST="${QDRANT_DIR}/${COLLECTION}-${TIMESTAMP}.tar.gz"
curl -fsSL \
    "${QDRANT_URL}/collections/${COLLECTION}/snapshots/${SNAPSHOT_NAME}" \
    -o "${QDRANT_DEST}"

QDRANT_SIZE=$(du -sh "${QDRANT_DEST}" | cut -f1)
QDRANT_SHA256=$(sha256sum "${QDRANT_DEST}" | cut -d' ' -f1)
echo "  Saved: ${QDRANT_DEST} (${QDRANT_SIZE})"
echo "  SHA256: ${QDRANT_SHA256}"

# Clean up the server-side snapshot to free Qdrant storage
echo "  Deleting server-side snapshot..."
curl -fsS -X DELETE \
    "${QDRANT_URL}/collections/${COLLECTION}/snapshots/${SNAPSHOT_NAME}" \
    > /dev/null

# ── 2. SQLite backup ─────────────────────────────────────────────────────────
echo "[2/3] Backing up SQLite jobs DB..."
if [[ ! -f "${SQLITE_PATH}" ]]; then
    echo "  WARNING: SQLite DB not found at ${SQLITE_PATH} — skipping." >&2
else
    SQLITE_DEST="${SQLITE_DIR}/jobs-${TIMESTAMP}.db"
    # sqlite3 .backup is safe under concurrent writers (uses WAL/shared-cache lock)
    sqlite3 "${SQLITE_PATH}" ".backup '${SQLITE_DEST}'"
    SQLITE_SIZE=$(du -sh "${SQLITE_DEST}" | cut -f1)
    SQLITE_SHA256=$(sha256sum "${SQLITE_DEST}" | cut -d' ' -f1)
    echo "  Saved: ${SQLITE_DEST} (${SQLITE_SIZE})"
    echo "  SHA256: ${SQLITE_SHA256}"
fi

# ── 3. Summary ───────────────────────────────────────────────────────────────
echo "[3/3] Backup complete."
echo ""
echo "Restore instructions:"
echo "  Qdrant: POST ${QDRANT_URL}/collections/${COLLECTION}/snapshots/recover"
echo "          body: {\"location\": \"file://${QDRANT_DEST}\"}"
echo "  SQLite: cp ${SQLITE_DEST} ${SQLITE_PATH}   (stop workers first)"
