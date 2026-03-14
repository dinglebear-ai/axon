#!/usr/bin/env bash
# Paginate through all indexed domains via axon-local MCP (stdio) and write to file.
set -euo pipefail

OUTPUT="${1:-/tmp/axon-domains.txt}"
BATCH=100
OFFSET=0
TOTAL=0

echo "Fetching all indexed domains → $OUTPUT"
: > "$OUTPUT"

while true; do
    RESULT=$(mcporter call axon-local.axon \
        action=domains \
        "limit=$BATCH" \
        "offset=$OFFSET" 2>/dev/null)

    # Extract domain names and append to output file
    echo "$RESULT" | jq -r '.data.data.domains[].domain' >> "$OUTPUT"

    # Count how many came back this page
    BATCH_COUNT=$(echo "$RESULT" | jq '.data.data.domains | length')

    TOTAL=$((TOTAL + BATCH_COUNT))
    echo "  offset=$OFFSET: got $BATCH_COUNT domains (running total: $TOTAL)"

    if [ "$BATCH_COUNT" -lt "$BATCH" ]; then
        break
    fi
    OFFSET=$((OFFSET + BATCH))
done

# Deduplicate and sort in place
sort -u "$OUTPUT" -o "$OUTPUT"
FINAL=$(wc -l < "$OUTPUT")
echo "Done. $FINAL unique domains written to $OUTPUT"
