#!/usr/bin/env bash
# Domain scenarios sourced by live-test-all-commands.sh.

handle_live_resources_graph_scenario() {
  local name="$1"
  case "$name" in
      "artifacts list")
        "$AXON_BIN" screenshot "$fixture_url?artifact_page=2" \
          --output "$OUTDIR/screenshot-2.png" --json \
          >"$OUTDIR/logs/fixture-screenshot-2.json" \
          2>"$OUTDIR/logs/fixture-screenshot-2.stderr.log"
        run_live "$name" artifacts list --kind screenshot --limit 1 --json
        artifacts_cursor="$(jq -r '.next_cursor // empty' "$LAST_LIVE_LOG")"
        if [ -n "$artifacts_cursor" ]; then
          run_live "$name" artifacts list --kind screenshot --cursor "$artifacts_cursor" --limit 1 --json
        else
          missing_live "artifacts list cursor" "fixture did not produce a second screenshot page"
        fi
        run_live "$name" artifacts list --source-id "$source_id" --json
        assert_live_json "artifacts source filter excludes unrelated screenshots" "$LAST_LIVE_LOG" \
          '.items | length == 0'
        run_live "$name" artifacts list --job-id "$job_id" --json
        assert_live_json "artifacts job filter excludes unrelated screenshots" "$LAST_LIVE_LOG" \
          '.items | length == 0'
        ;;
      "artifacts get")
        run_live "$name" artifacts get "$screenshot_artifact_id" --include-content-url --json
        assert_live_json "artifact content URL projection" "$LAST_LIVE_LOG" \
          '(.content_url | type) == "string" and (.content_url | length) > 0
           and .metadata.full_page == false'
        prove_option_behavior "@global" "--screenshot-full-page" \
          "stored screenshot metadata reported viewport-only capture"
        ;;
      "artifacts content")
        run_live "$name" artifacts content "$screenshot_artifact_id" --output "$OUTDIR/artifact-content.png" --json
        run_live "$name" artifacts content "$screenshot_artifact_id" \
          --range bytes=0-15 --output "$OUTDIR/artifact-range.bin" --json
        assert_live_json "artifact byte range receipt" "$LAST_LIVE_LOG" \
          '.size_bytes == 16'
        if [ "$(wc -c <"$OUTDIR/artifact-range.bin")" -eq 16 ]; then
          prove_option_behavior "artifacts content" "--range" \
            "requested inclusive byte range wrote exactly 16 bytes"
        else
          missing_live "artifact byte range file" "range output was not exactly 16 bytes"
        fi
        pushd "$OUTDIR" >/dev/null || exit 1
        run_live "$name" artifacts content "$screenshot_artifact_id" --download --json
        popd >/dev/null || exit 1
        # shellcheck disable=SC2016
        assert_live_json "artifact default download path" "$LAST_LIVE_LOG" \
          --arg expected "$screenshot_artifact_id.png" \
          '.output == $expected and .size_bytes > 0'
        artifact_download_name="$(jq -r '.output // empty' "$LAST_LIVE_LOG")"
        if [ -n "$artifact_download_name" ] \
          && [ -s "$OUTDIR/$artifact_download_name" ]; then
          prove_option_behavior "artifacts content" "--download" \
            "download mode wrote the artifact-id-derived default filename"
        else
          missing_live "artifact default download file" "download did not create a non-empty default file"
        fi
        ;;
      "uploads list")
        "$AXON_BIN" uploads create /etc/hosts --purpose source_artifact --json >"$OUTDIR/logs/fixture-upload-complete.json" 2>"$OUTDIR/logs/fixture-upload-complete.stderr.log"
        upload_id="$(jq -r '.upload.upload_id // .status.upload_id // empty' "$OUTDIR/logs/fixture-upload-complete.json")"
        "$AXON_BIN" uploads create /etc/hosts --purpose source_artifact --json >"$OUTDIR/logs/fixture-upload-abort.json" 2>"$OUTDIR/logs/fixture-upload-abort.stderr.log"
        abort_upload_id="$(jq -r '.upload.upload_id // .status.upload_id // empty' "$OUTDIR/logs/fixture-upload-abort.json")"
        run_live "$name" uploads list --status received --limit 1 --json
        uploads_cursor="$(jq -r '.next_cursor // empty' "$LAST_LIVE_LOG")"
        if [ -n "$uploads_cursor" ]; then
          run_live "$name" uploads list --status received --cursor "$uploads_cursor" --limit 1 --json
        else
          missing_live "uploads list cursor" "fixture did not produce a second received upload page"
        fi
        ;;
      "uploads create")
        run_live "$name" uploads create /etc/hosts --purpose source_artifact --source-hint live-smoke --json
        ;;
      "uploads get") run_live "$name" uploads get "$upload_id" --json ;;
      "uploads complete")
        upload_sha256="$(sha256sum /etc/hosts | awk '{print $1}')"
        run_live "$name" uploads complete "$upload_id" --sha256 "$upload_sha256" \
          --source-option "scope=page" --json
        ;;
      "uploads abort") run_live "$name" uploads abort "$abort_upload_id" --reason "live harness cleanup" --json ;;
      "collections list") run_live "$name" collections list --json ;;
      "collections get") run_live "$name" collections get "$AXON_COLLECTION" --include-schema --include-indexes --json ;;
      "graph kinds") run_live "$name" graph kinds --json ;;
      "graph resolve")
        run_live "$name" graph resolve "$fixture_url" --kind web_origin --limit 10 --json
        "$AXON_BIN" graph source "src_0272b3e7006f0910" --depth 2 --limit 100 --json >"$OUTDIR/logs/fixture-graph-source.json" 2>"$OUTDIR/logs/fixture-graph-source.stderr.log" || true
        graph_node_id="$(jq -r '.. | .node_id? // empty' "$OUTDIR/logs/fixture-graph-source.json" 2>/dev/null | head -1)"
        graph_edge_id="$(jq -r '.. | .edge_id? // empty' "$OUTDIR/logs/fixture-graph-source.json" 2>/dev/null | head -1)"
        ;;
      "graph query")
        run_live "$name" graph query "$fixture_url" --limit 1 --json
        graph_cursor="$(jq -r '.next_cursor // empty' "$LAST_LIVE_LOG")"
        if [ -n "$graph_cursor" ]; then
          run_live "$name" graph query "$fixture_url" --cursor "$graph_cursor" --limit 1 --json
        else
          missing_live "graph query cursor" "fixture traversal did not produce a second edge page"
        fi
        ;;
      "graph node")
        run_live "$name" graph node "$graph_node_id" --include-edges --include-evidence --json
        assert_live_json "graph node evidence projection" "$LAST_LIVE_LOG" \
          'any(.edges[]; (.evidence | length) > 0)'
        ;;
      "graph edge")
        run_live "$name" graph edge "$graph_edge_id" --include-evidence --json
        assert_live_json "graph edge evidence projection" "$LAST_LIVE_LOG" \
          '(.evidence | length) > 0'
        ;;
      "graph source") run_live "$name" graph source "src_0272b3e7006f0910" --depth 2 --edge-kind docs_site_contains_page --limit 100 --json ;;
      "providers list")
        run_live "$name" providers list --kind qdrant --status healthy --json
        assert_live_json "providers list filtered qdrant" "$LAST_LIVE_LOG" \
          '.providers | length == 1 and .[0].id == "qdrant" and .[0].ok == true'
        ;;
      "providers get")
        run_live "$name" providers get qdrant --include-health --include-limits --json
        assert_live_json "providers get qdrant identity" "$LAST_LIVE_LOG" \
          '.id == "qdrant" and .ok == true'
        while IFS= read -r provider; do
          [ "$provider" = "qdrant" ] && continue
          run_live "providers get ($provider)" providers get "$provider" --include-health --include-limits --json
        done < <(jq -r '.providers[]?.id' "$OUTDIR/logs/live-providers_list.log")
        ;;
      "capabilities") run_live "$name" capabilities --json ;;
      "chat") run_live "$name" chat --query "Reply with exactly: axon live chat ok" --json ;;
    *) return 1 ;;
  esac
  return 0
}
