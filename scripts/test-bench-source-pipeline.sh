#!/bin/bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
export AXON_BENCH_LIBRARY_MODE=1
# shellcheck source=bench-source-pipeline.sh
source "$SCRIPT_DIR/bench-source-pipeline.sh"

fail() { echo "FAIL: $*" >&2; exit 1; }
validate_job_id '123e4567-e89b-12d3-a456-426614174000' || fail valid-job-id
! validate_job_id "x' OR 1=1 --" || fail injected-job-id
validate_safe_source 'https://example.invalid/path with spaces?q="quoted"' || fail safe-source
! validate_safe_source 'https://user:password@example.invalid/' || fail userinfo-source
! validate_safe_source 'https://example.invalid/$(touch /tmp/nope)' || fail substitution-source

sanitized=$(printf '%s\n' 'Authorization:Bearer-secret https://user:pass@example.test abcdefghijklmnopqrstuvwxyz123456' | sanitize_text)
[[ $sanitized != *Bearer-secret* ]] || fail bearer-redaction
[[ $sanitized != *example.test* ]] || fail url-redaction
[[ $sanitized != *abcdefghijklmnopqrstuvwxyz123456* ]] || fail long-value-redaction

grep -q 'umask 077' "$SCRIPT_DIR/bench-source-pipeline.sh" || fail umask
grep -q 'mktemp -d' "$SCRIPT_DIR/bench-source-pipeline.sh" || fail private-temp
! grep -Eq '(^|[[:space:]])set -x([[:space:]]|$)' "$SCRIPT_DIR/bench-source-pipeline.sh" || fail xtrace
! grep -Eq '(^|[[:space:]])eval([[:space:]]|$)' "$SCRIPT_DIR/bench-source-pipeline.sh" || fail dynamic-evaluation

echo 'bench-source-pipeline tests passed'
