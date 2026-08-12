#!/usr/bin/env bash
# Domain scenarios sourced by live-test-all-commands.sh.

handle_live_jobs_memory_source_scenario() {
  local name="$1"
  case "$name" in
      "status")
        export AXON_DATA_DIR="$OUTDIR/jobs-data"
        mkdir -p "$AXON_DATA_DIR"
        "$AXON_BIN" source "$fixture_url" --scope page --wait true --collection "$AXON_COLLECTION" --json >"$OUTDIR/logs/fixture-job.json" 2>"$OUTDIR/logs/fixture-job.stderr.log"
        job_id="$(jq -r '.job_id // .job.id // empty' "$OUTDIR/logs/fixture-job.json")"
        source_id="$(jq -r '.source_id // empty' "$OUTDIR/logs/fixture-job.json")"
        run_live "$name" status --json
        # shellcheck disable=SC2016
        assert_live_json "status completed counts" "$LAST_LIVE_LOG" \
          --arg job_id "$job_id" \
          '.jobs[] | select(.job_id == $job_id) | (.result.documents_done > 0 and .result.chunks_done > 0)'
        "$AXON_BIN" status >"$OUTDIR/logs/live-status-human.log" 2>"$OUTDIR/logs/live-status-human.stderr.log"
        assert_live_text "status completed details" "$OUTDIR/logs/live-status-human.log" " docs · 100% · "
        run_live "$name" status --active --color never --quiet --json
        run_live "$name" status --recent --color never --quiet --json
        run_live "$name" status --reclaimed --color never --quiet --json
        ;;
      "jobs list")
        run_live "$name" jobs list --json
        # shellcheck disable=SC2016
        assert_live_json "jobs list completed counts" "$LAST_LIVE_LOG" \
          --arg job_id "$job_id" \
          '.items[] | select(.job_id == $job_id) | (.counts.documents_done > 0 and .counts.chunks_done > 0)'
        "$AXON_BIN" source "$fixture_url?jobs_page=2" --scope page --wait true \
          --collection "$AXON_COLLECTION" --json \
          >"$OUTDIR/logs/fixture-job-page-2.json" \
          2>"$OUTDIR/logs/fixture-job-page-2.stderr.log"
        run_live "$name" jobs list --status completed --kind source --limit 1 --json
        jobs_cursor="$(jq -r '.next_cursor // empty' "$LAST_LIVE_LOG")"
        if [ -n "$jobs_cursor" ]; then
          run_live "$name" jobs list --status completed --kind source \
            --limit 1 --cursor "$jobs_cursor" --json
        else
          missing_live "jobs list cursor" "fixture did not produce a second filtered job page"
        fi
        ;;
      "jobs get")
        run_live "$name" jobs get "$job_id" --json
        assert_live_json "jobs get completed counts" "$LAST_LIVE_LOG" \
          '.counts.documents_done > 0 and .counts.chunks_done > 0'
        "$AXON_BIN" jobs get "$job_id" >"$OUTDIR/logs/live-jobs_get-human.log" 2>"$OUTDIR/logs/live-jobs_get-human.stderr.log"
        assert_live_text "jobs get completed details" "$OUTDIR/logs/live-jobs_get-human.log" "Result:"
        assert_live_text "jobs get completed totals" "$OUTDIR/logs/live-jobs_get-human.log" " docs · 100% · "
        ;;
      "jobs events")
        run_live "$name" jobs events "$job_id" --after-sequence 0 --limit 1 --json
        jobs_events_cursor="$(jq -r '.next_cursor // empty' "$LAST_LIVE_LOG")"
        if [ -n "$jobs_events_cursor" ]; then
          run_live "$name" jobs events "$job_id" --cursor "$jobs_events_cursor" --limit 1 --json
        else
          missing_live "jobs events cursor" "fixture did not produce a second event page"
        fi
        ;;
      "jobs stream") run_live "$name" jobs stream "$job_id" --after-sequence 0 --limit 100 --json ;;
      "jobs cancel")
        "$AXON_BIN" source "$fixture_url?axon_cancel_fixture=$$" --scope page --wait false \
          --collection "$AXON_COLLECTION" --json \
          >"$OUTDIR/logs/fixture-job-cancel.json" \
          2>"$OUTDIR/logs/fixture-job-cancel.stderr.log"
        job_id="$(jq -r '.job_id // .job.id // empty' "$OUTDIR/logs/fixture-job-cancel.json")"
        run_live "$name" jobs cancel "$job_id" --reason "live harness cancellation" --json
        ;;
      "jobs retry") run_live "$name" jobs retry "$job_id" --mode same_config --json ;;
      "jobs recover") run_live "$name" jobs recover --kind source --stale-before "2026-01-01T00:00:00Z" --json ;;
      "jobs cleanup") run_live "$name" jobs cleanup --status completed --kind source --older-than "2099-01-01T00:00:00Z" --dry-run --json ;;
      "jobs clear") run_live "$name" jobs clear --confirm --json ;;
      "jobs worker")
        run_live "$name" jobs worker --idle-exit-secs 2 --json
        export AXON_DATA_DIR="$main_data_dir"
        ;;
      "memory remember")
        run_live "$name" memory remember "Axon live CLI fixture memory" \
          --title "Live fixture" --type fact --project "axon-live-cli" \
          --repo "dinglebear-ai/axon" --file "scripts/live-test-all-commands.sh" \
          --confidence 0.92 --json
        memory_id="$(jq -r '.memory.id // .id // empty' "$LAST_LIVE_LOG" 2>/dev/null)"
        "$AXON_BIN" memory remember "Axon replacement fixture memory" --title "Replacement fixture" --project "axon-live-cli" --json >"$OUTDIR/logs/fixture-memory.json" 2>"$OUTDIR/logs/fixture-memory.stderr.log"
        replacement_memory_id="$(jq -r '.memory.id // .id // empty' "$OUTDIR/logs/fixture-memory.json")"
        ;;
      "memory list")
        run_live "$name" memory list --project "axon-live-cli" --repo "dinglebear-ai/axon" \
          --file "scripts/live-test-all-commands.sh" --type fact --status active --limit 5 --json
        ;;
      "memory search")
        run_live "$name" memory search "fixture memory" --project "axon-live-cli" \
          --repo "dinglebear-ai/axon" --file "scripts/live-test-all-commands.sh" \
          --limit 5 --json
        ;;
      "memory show") run_live "$name" memory show "$memory_id" --json ;;
      "memory link") run_live "$name" memory link "$memory_id" "$replacement_memory_id" --type relates_to --json ;;
      "memory supersede") run_live "$name" memory supersede "$replacement_memory_id" "$memory_id" --json ;;
      "memory context")
        run_live "$name" memory context --query fixture --project "axon-live-cli" \
          --repo "dinglebear-ai/axon" --file "scripts/live-test-all-commands.sh" \
          --limit 5 --token-budget 1000 --json
        ;;
      "sessions") run_live "$name" sessions --claude --codex --gemini --project "axon-cli-nonexistent-project-zz" --wait true --json ;;
      "source")
        run_live "$name" source "$fixture_url" --scope page --max-pages 1 --max-depth 0 --include-subdomains false --render-mode chrome --cache true --format json --limit 2 --batch-concurrency 1 --wait true --performance-profile high-stable --normalize true --block-assets true --chrome-wait-for-selector body --root-selector body --exclude-selector ".axon-never" --header "X-Axon-Live: enabled" --budget "*=1" --collection "$AXON_COLLECTION" --json
        assert_live_json "source completed content" "$LAST_LIVE_LOG" \
          '.status == "completed" and .documents_prepared > 0 and .vector_points_written > 0'
        prove_options "source" "source completed through the requested page scope" --scope
        prove_options "@global" \
          "source completed with the requested acquisition, preparation, and publication controls" \
          --max-pages --max-depth --include-subdomains --render-mode --cache --format \
          --limit --batch-concurrency --wait --performance-profile --normalize \
          --block-assets --chrome-wait-for-selector --root-selector --exclude-selector \
          --header --budget --collection
        jq -n '{"/":[{"action":"wait","ms":25}]}' >"$OUTDIR/automation-script.json"
        run_live "$name" source "$fixture_url?automation=$$" --scope site \
          --max-pages 1 --max-depth 0 --render-mode chrome \
          --automation-script "$OUTDIR/automation-script.json" \
          --chrome-screenshot true --output-dir "$OUTDIR/source-output" \
          --warc "$OUTDIR/source-output/live.warc" --skip-embed \
          --wait true --collection "$AXON_COLLECTION" --json
        assert_live_json "source automation artifact-only crawl" "$LAST_LIVE_LOG" \
          '.status == "completed" and .documents_prepared > 0
           and .vector_points_written == 0'
        warc_artifact="$(find "$OUTDIR/source-output/artifacts" -maxdepth 1 \
          -type f -name 'art_warc_*.bin' -print -quit 2>/dev/null)"
        if [ -n "$warc_artifact" ] && [ -s "$warc_artifact" ] \
          && head -1 "$warc_artifact" | grep -Fq "WARC/1.1"; then
          record "source WARC archive" "contract" "PASS" "0" \
            "non-empty WARC/1.1 artifact" "$warc_artifact"
          prove_options "@global" \
            "Chrome crawl loaded the automation script, captured a screenshot, saved output, wrote WARC, and skipped embeddings" \
            --automation-script --chrome-screenshot --output-dir --warc --skip-embed
        else
          missing_live "source WARC archive" "crawl did not create a valid WARC/1.1 archive"
        fi
        run_live "$name" source "$fixture_url?cache_fixture=$$" --scope page \
          --render-mode http --cache true --cache-http-only --etag-conditional \
          --output-dir "$OUTDIR/cache-output" --skip-embed --wait true \
          --collection "$AXON_COLLECTION" --json
        assert_live_json "source conditional HTTP cache first pass" "$LAST_LIVE_LOG" \
          '(.status == "completed"
            or (.status == "completed_degraded"
                and any(.warnings[]; .code == "parse.unsupported")))
           and .vector_points_written == 0'
        run_live "$name" source "$fixture_url?cache_fixture=$$" --scope page \
          --render-mode http --cache true --cache-http-only --etag-conditional \
          --output-dir "$OUTDIR/cache-output" --skip-embed --wait true \
          --collection "$AXON_COLLECTION" --json
        assert_live_json "source conditional HTTP cache reuse" "$LAST_LIVE_LOG" \
          '.status == "completed" and .vector_points_written == 0'
        prove_options "@global" \
          "repeat HTTP crawl completed through the cache-only conditional request path" \
          --cache-http-only --etag-conditional
        local_fixture="$OUTDIR/local-source"
        mkdir -p "$local_fixture/keep" "$local_fixture/vendor" "$local_fixture/docs/private"
        printf '# Keep\nAxon live included fixture.\n' >"$local_fixture/keep/keep.md"
        printf '# Excluded\nVendor fixture.\n' >"$local_fixture/vendor/excluded.md"
        printf '# Excluded\nPrivate fixture.\n' >"$local_fixture/docs/private/excluded.md"
        run_live "$name" source "$local_fixture" \
          --exclude-path vendor/ --exclude-path docs/private/ \
          --skip-embed --wait true --collection "$AXON_COLLECTION" --json
        assert_live_json "source local path exclusions" "$LAST_LIVE_LOG" \
          '.status == "completed" and .documents_prepared == 1
           and .vector_points_written == 0'
        prove_option_behavior "@global" "--exclude-path" \
          "local source prepared only the included file while excluding both requested paths"
        run_live "source URL glob" source \
          --url-glob "https://www.rust-lang.org/{learn,tools}" \
          --scope page --skip-embed --batch-concurrency 2 --wait true \
          --collection "$AXON_COLLECTION" --json
        assert_live_json "source URL glob expansion" "$LAST_LIVE_LOG" \
          '.count == 2 and .succeeded == 2 and .failed == 0'
        record_behavior_args "source" "${PREPARED_ARGS[@]}"
        prove_option_behavior "@global" "--url-glob" \
          "brace expansion produced two successful source members"
        run_live "$name" source "$fixture_url?cron=$$" --scope page \
          --cron-every-seconds 1 --cron-max-runs 2 --skip-embed --wait true \
          --collection "$AXON_COLLECTION" --json
        prove_options "@global" \
          "bounded cron source emitted two completed runs and then exited" \
          --cron-every-seconds --cron-max-runs
        mixed_success_url="$fixture_url?axon_mixed_success=$$"
        run_live_expect_failure "source mixed batch" source \
          --urls "$mixed_success_url,http://127.0.0.1:1/axon-live-unreachable" \
          --scope page --batch-concurrency 2 --wait true --collection "$AXON_COLLECTION" --json
        mixed_failure_stderr="${LAST_LIVE_LOG%.log}.stderr.log"
        assert_live_json "source mixed batch reports intended failure" "$LAST_LIVE_LOG" \
          --arg failed_input "http://127.0.0.1:1/axon-live-unreachable" \
          '.count == 2 and .succeeded == 1 and .failed == 1
           and ([.results[] | select(.status == "completed")] | length) == 1
           and ([.results[] | select(.status == "failed")] | length) == 1
           and any(.results[];
             .status == "failed"
             and .input == $failed_input
             and .error == "web source indexing failed"
             and (has("job_id") | not))'
        assert_live_text "source mixed batch aggregate failure" "$mixed_failure_stderr" \
          "Error: 1 of 2 source inputs failed"
        prove_option_behavior "@global" "--urls" "mixed batch reported both inputs and the intended failed member"
        prove_option_behavior "@global" "--batch-concurrency" "mixed siblings both reached terminal outcomes"
        prove_option_behavior "@global" "--wait" "command returned only after both batch outcomes were terminal"
        mixed_success_job_id="$(jq -r \
          '.results[] | select(.status == "completed") | .job_id // empty' \
          "$LAST_LIVE_LOG" | head -1)"
        "$AXON_BIN" jobs get "$mixed_success_job_id" --json \
          >"$OUTDIR/logs/fixture-mixed-job.json" \
          2>"$OUTDIR/logs/fixture-mixed-job.stderr.log"
        assert_live_json "source mixed batch drains successful sibling" \
          "$OUTDIR/logs/fixture-mixed-job.json" \
          '.status == "completed" and .counts.documents_done > 0'
        ;;
      "screenshot")
        run_live "$name" --screenshot-full-page false --viewport 1280x720 \
          screenshot "$fixture_url" --output "$OUTDIR/screenshot.png" --json
        assert_live_json "screenshot artifact contract" "$LAST_LIVE_LOG" \
          '.artifact_id != null and .artifact_id != ""
           and .width == 1280 and .height == 720'
        if [ "$(od -An -tx1 -N8 "$OUTDIR/screenshot.png" 2>/dev/null | tr -d ' \n')" = "89504e470d0a1a0a" ]; then
          prove_option_behavior "screenshot" "--output" "requested screenshot path contains a non-empty PNG"
          prove_option_behavior "@global" "--viewport" "capture returned dimensions for the requested viewport"
          prove_option_behavior "@global" "--screenshot-full-page" \
            "capture returned the requested viewport height instead of a full-page image"
          record "screenshot output file" "contract" "PASS" "0" \
            "PNG signature at $OUTDIR/screenshot.png" "$OUTDIR/screenshot.png"
        else
          failures=$((failures + 1))
          record "screenshot output file" "contract" "FAIL" "1" \
            "PNG signature at $OUTDIR/screenshot.png" "$OUTDIR/screenshot.png"
        fi
        ;;
      "completions")
        run_live "$name" completions bash
        assert_live_nonempty "completions bash output" "$LAST_LIVE_LOG"
        cp "$LAST_LIVE_LOG" "$OUTDIR/logs/completions-bash-primary.log"
        run_live "completion alias" completion bash
        assert_live_nonempty "completion alias output" "$LAST_LIVE_LOG"
        if cmp -s "$OUTDIR/logs/completions-bash-primary.log" "$LAST_LIVE_LOG"; then
          record "completion alias equivalence" "contract" "PASS" "0" \
            "completion bash == completions bash" "$LAST_LIVE_LOG"
          record_behavior_args "completion alias" completion bash
        else
          failures=$((failures + 1))
          record "completion alias equivalence" "contract" "FAIL" "1" \
            "completion bash == completions bash" "$LAST_LIVE_LOG"
        fi
        "$AXON_BIN" completions zsh >"$OUTDIR/logs/completions-zsh.log" 2>&1 || failures=$((failures + 1))
        "$AXON_BIN" completions fish >"$OUTDIR/logs/completions-fish.log" 2>&1 || failures=$((failures + 1))
        ;;
    *) return 1 ;;
  esac
  return 0
}
