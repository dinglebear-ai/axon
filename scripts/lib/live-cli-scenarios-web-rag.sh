#!/usr/bin/env bash
# Domain scenarios sourced by live-test-all-commands.sh.

handle_live_web_rag_scenario() {
  local name="$1"
  case "$name" in
      "watch create")
        run_live "$name" watch create "$fixture_url" --every-seconds 3600 --collection "$AXON_COLLECTION" --json
        watch_id="$(jq -r '.watch_id // empty' "$LAST_LIVE_LOG" 2>/dev/null)"
        ;;
      "watch list") run_live "$name" watch list --json ;;
      "watch get") run_live "$name" watch get "$watch_id" --json ;;
      "watch status") run_live "$name" watch status "$watch_id" --json ;;
      "watch update") run_live "$name" watch update "$watch_id" --every-seconds 7200 --collection "$AXON_COLLECTION" --json ;;
      "watch exec") run_live "$name" watch exec "$watch_id" --json ;;
      "watch pause") run_live "$name" watch pause "$watch_id" --json ;;
      "watch resume") run_live "$name" watch resume "$watch_id" --json ;;
      "watch delete")
        run_live "watch history" watch history "$watch_id" --json
        run_live "$name" watch delete "$watch_id" --json
        ;;
      "watch history") : ;;
      "monitor jobs")
        run_live "$name" monitor jobs --json
        run_live_monitor_jsonl "monitor jobs watch JSONL"
        ;;
      "map")
        run_live "$name" map "$map_fixture_url" --max-pages 10 --json
        assert_live_json "map discovered URLs" "$LAST_LIVE_LOG" \
          '.map_source != null and (.urls | length) > 0'
        prove_option_behavior "@global" "--max-pages" "map returned a bounded non-empty URL set"
        prove_option_behavior "@global" "--json" "map stdout parsed as the map result object"
        run_live "$name" map "https://gofastmcp.com/" --sitemap-only \
          --exclude-path-prefix /apps --max-pages 25 --json
        assert_live_json "map sitemap path exclusion" "$LAST_LIVE_LOG" \
          '.map_source != null and (.urls | length) > 0
           and all(.urls[]; (startswith("https://gofastmcp.com/apps") | not))'
        timeout "${TIMEOUT_SECS}s" "$AXON_BIN" source "$map_fixture_url" --scope map --wait true \
          --collection "$AXON_COLLECTION" --json \
          >"$OUTDIR/logs/fixture-map-job.json" \
          2>"$OUTDIR/logs/fixture-map-job.stderr.log"
        map_job_id="$(jq -r '.job_id // .job.id // empty' "$OUTDIR/logs/fixture-map-job.json")"
        run_live "jobs get (map item-only)" jobs get "$map_job_id" --json
        assert_live_json "jobs get map item counts" "$LAST_LIVE_LOG" \
          '.counts.items_done > 0 and .counts.documents_done == 0 and .counts.chunks_done == 0'
        timeout "${TIMEOUT_SECS}s" "$AXON_BIN" jobs get "$map_job_id" \
          >"$OUTDIR/logs/live-jobs_get-map-human.log" \
          2>"$OUTDIR/logs/live-jobs_get-map-human.stderr.log"
        assert_live_text "jobs get map item details" \
          "$OUTDIR/logs/live-jobs_get-map-human.log" " items · 100%"
        ;;
      "endpoints")
        run_live "$name" endpoints "$fixture_url" \
          --include-bundles true --first-party-only true --unique-only true \
          --max-scripts 2 --max-scan-bytes 1000000 --verify --capture-network \
          --probe-rpc --probe-rpc-subdomains --json
        assert_live_json "endpoints option behavior" "$LAST_LIVE_LOG" \
          '(.endpoints | length) > 0
           and all(.endpoints[]; .first_party == true)
           and (([.endpoints[].normalized_url] | unique | length) == (.endpoints | length))
           and any(.endpoints[]; .source == "network_capture")
           and any(.endpoints[]; .verified.reachable == true)
           and any(.mcp_candidates[]; .host_kind == "apex_subdomain")'
        ;;
      "extract")
        run_live "$name" extract "$fixture_url" --query "extract the page title" --wait false --json
        extract_job_id="$(jq -r '.job_id // .job.id // empty' "$LAST_LIVE_LOG" 2>/dev/null)"
        ;;
      "extract status") run_live "$name" extract status "$extract_job_id" --json ;;
      "extract cancel") run_live "$name" extract cancel "$extract_job_id" --json ;;
      "extract errors") run_live "$name" extract errors "$extract_job_id" --json ;;
      "extract list") run_live "$name" extract list --json ;;
      "extract cleanup") run_live "$name" extract cleanup --json ;;
      "extract clear") run_live "$name" extract clear --json ;;
      "extract worker") run_live "$name" extract worker --json ;;
      "extract recover") run_live "$name" extract recover --json ;;
      "search")
        run_live "$name" search "OpenAI GPT-5.4 release" \
          --limit 1 --search-time-range day --json
        assert_live_json "search recent time filter" "$LAST_LIVE_LOG" \
          '(.results | length) > 0'
        ;;
      "research")
        run_live "$name" research "FastMCP official documentation" \
          --research-depth 1 --json
        assert_live_json "research synthesized official documentation" "$LAST_LIVE_LOG" \
          '(.summary | type) == "string" and (.summary | length) > 0
           and (.search_results | length) > 0'
        ;;
      "scrape")
        run_live "$name" scrape "$fixture_url" --wait true --collection "$AXON_COLLECTION" --json
        run_live "$name" scrape "$fixture_url?no_embed=$$" --no-embed --inline \
          --output-dir "$OUTDIR/scrape-output" --wait true \
          --collection "$AXON_COLLECTION" --json
        assert_live_json "scrape no-embed inline projection" "$LAST_LIVE_LOG" \
          '.status == "completed" and .vector_points_written == 0
           and (.inline.content.text | length) > 0'
        ;;
      "brand") run_live "$name" brand "$fixture_url" --json ;;
      "debug") run_live "$name" debug --json ;;
      "diff") run_live "$name" diff "$fixture_url" "https://www.iana.org/help/example-domains" --json ;;
      "doctor")
        run_live "$name" doctor --json
        assert_live_json "doctor providers healthy" "$LAST_LIVE_LOG" \
          '.all_ok == true'
        ;;
      "doctor diagnose") run_live "$name" doctor diagnose --json ;;
      "query")
        run_live "$name" query "Example Domain" --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "query returns ranked result" "$LAST_LIVE_LOG" \
          'select(.rank == 1 and .url != null and .snippet != null)'
        run_live "$name" query --query "Example Domain" --diagnostics \
          --no-hybrid-search --since 30d --before 2099-01-01 \
          --tei-url "$TEI_URL" --qdrant-url "$QDRANT_URL" \
          --limit 2 --collection "$AXON_COLLECTION" --color never --quiet --json
        assert_live_json "query explicit dense diagnostic projection" "$LAST_LIVE_LOG" \
          'select(.rank == 1 and .url != null and .snippet != null)'
        ;;
      "retrieve")
        run_live "$name" retrieve "$fixture_url" --max-points 10 --collection "$AXON_COLLECTION" --json
        assert_live_json "retrieve returns indexed content" "$LAST_LIVE_LOG" \
          '.chunks > 0 and (.content | length) > 0'
        ;;
      "ask")
        run_live "$name" ask "What is the Example Domain used for?" \
          --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask returns grounded answer" "$LAST_LIVE_LOG" \
          '(.answer | length) > 0 and (.citations | length) > 0'
        run_live "$name" ask "Restate that purpose in one sentence." \
          --continue --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask continue alias" "$LAST_LIVE_LOG" \
          '(.answer | length) > 0'
        prove_option_behavior "ask" "--continue" "follow-up answer was generated from prior session context"
        run_live "$name" ask "Restate that purpose in five words." \
          --follow-up --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask follow-up long flag" "$LAST_LIVE_LOG" \
          '(.answer | length) > 0'
        prove_option_behavior "ask" "--follow-up" "long follow-up flag generated a non-empty answer"
        run_live "$name" ask "Restate that purpose in three words." \
          -c --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask follow-up short flag" "$LAST_LIVE_LOG" \
          '(.answer | length) > 0'
        prove_option_behavior "ask" "-c" "short follow-up alias generated a non-empty answer"
        run_live "ask session create" ask "State the Example Domain purpose." \
          --session axon-live-session --new-session --no-stream \
          --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask named session create" "$LAST_LIVE_LOG" \
          '.session == "axon-live-session" and (.answer | length) > 0'
        record_behavior_args "ask" "${PREPARED_ARGS[@]}"
        run_live "ask session resume stream" ask "Repeat it more briefly." \
          --resume axon-live-session --stream \
          --limit 2 --collection "$AXON_COLLECTION"
        assert_live_text "ask resumed human stream" "$LAST_LIVE_LOG" "Assistant:"
        record_behavior_args "ask" "${PREPARED_ARGS[@]}"
        run_live "ask session reset" ask "Start over with the same factual answer." \
          --session axon-live-session --reset-session --no-stream \
          --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask named session reset" "$LAST_LIVE_LOG" \
          '.session == "axon-live-session" and (.answer | length) > 0'
        record_behavior_args "ask" "${PREPARED_ARGS[@]}"
        run_live "ask explanation" ask "Explain the retrieval plan for Example Domain." \
          --diagnostics --explain --no-stream \
          --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask explanation diagnostics" "$LAST_LIVE_LOG" \
          '.answer == "" and (.diagnostics | type) == "object"
           and (.explain | type) == "object"
           and .explain.mode == "explain_only"'
        record_behavior_args "ask" "${PREPARED_ARGS[@]}"
        run_live "ask list sessions" ask --list-sessions --json
        assert_live_json "ask list named session" "$LAST_LIVE_LOG" \
          'any(.[]; .name == "axon-live-session" and .turn_count > 0)'
        record_behavior_args "ask" "${PREPARED_ARGS[@]}"
        prove_mapped_command_options "ask"
        ;;
      "summarize") run_live "$name" summarize "$fixture_url" --collection "$AXON_COLLECTION" --json ;;
      "evaluate") run_live "$name" evaluate "What is the Example Domain used for?" --diagnostics --retrieval-ab --responses-mode side-by-side --collection "$AXON_COLLECTION" --json ;;
      "train") run_live "$name" train "Example Domain" --best 1 --notes "live CLI smoke" --collection "$AXON_COLLECTION" --json ;;
      "suggest") run_live "$name" suggest "Example Domain documentation" --limit 2 --collection "$AXON_COLLECTION" --json ;;
      "sources") run_live "$name" sources --all --domain example.com --json ;;
      "domains") run_live "$name" domains --domain example.com --json ;;
      "stats") run_live "$name" stats --json ;;
    *) return 1 ;;
  esac
  return 0
}
