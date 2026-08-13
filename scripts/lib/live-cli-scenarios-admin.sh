#!/usr/bin/env bash
# Domain scenarios sourced by live-test-all-commands.sh.

handle_live_admin_setup_scenario() {
  local name="$1"
  case "$name" in
      "serve") run_live_server "$name" "$LIVE_SERVE_PORT" serve ;;
      "serve mcp") run_live_server "$name" "$LIVE_MCP_SERVE_PORT" serve mcp --transport http ;;
      "reset")
        reset_data="$OUTDIR/reset-data"
        mkdir -p "$reset_data"
        saved_data_dir="$AXON_DATA_DIR"
        export AXON_DATA_DIR="$reset_data"
        run_live "$name" reset --stores jobs --json
        reset_plan_log="$OUTDIR/logs/reset-plan.log"
        cp "$LAST_LIVE_LOG" "$reset_plan_log"
        assert_live_json "reset plan is isolated and reviewable" "$reset_plan_log" \
          '.dry_run == true
           and .execution_state == "Planned"
           and .stores == ["jobs", "ledger", "code_index", "watch", "graph", "memory"]
           and (.stores | index("vectors") | not)
           and (.stores | index("artifacts") | not)
           and (.plan_id | length) > 0'
        if jq -e \
          '.dry_run == true
           and .execution_state == "Planned"
           and .stores == ["jobs", "ledger", "code_index", "watch", "graph", "memory"]
           and (.plan_id | length) > 0' \
          "$reset_plan_log" >/dev/null 2>&1; then
          reset_plan_id="$(jq -r '.plan_id' "$reset_plan_log")"
          prove_option_behavior "reset" "--stores" \
            "jobs scope expanded only to the six logical owners of the shared SQLite file"
          run_live "$name" reset --stores jobs --dry-run --yes --json
          assert_live_json "reset explicit dry-run pins preview mode" "$LAST_LIVE_LOG" \
            '.dry_run == true
             and .execution_state == "Planned"
             and .stores == ["jobs", "ledger", "code_index", "watch", "graph", "memory"]'
          prove_option_behavior "reset" "--dry-run" \
            "explicit dry-run remained non-destructive even alongside --yes"
          if jq -e \
            '.dry_run == true
             and .execution_state == "Planned"
             and .stores == ["jobs", "ledger", "code_index", "watch", "graph", "memory"]' \
            "$LAST_LIVE_LOG" >/dev/null 2>&1; then
            run_live "$name" reset --stores jobs --plan-id "$reset_plan_id" --yes --json
            cp "$LAST_LIVE_LOG" "$OUTDIR/logs/reset-exec.log"
            # shellcheck disable=SC2016
            assert_live_json "reset execution receipt" "$OUTDIR/logs/reset-exec.log" \
              --arg plan_id "$reset_plan_id" \
              '.dry_run == false
               and .execution_state == "Completed"
               and .plan_id == $plan_id
               and .stores == ["jobs", "ledger", "code_index", "watch", "graph", "memory"]
               and (.receipt_path | length) > 0'
            prove_option_behavior "reset" "--plan-id" \
              "execution returned the same reviewed plan id"
            prove_option_behavior "@global" "--yes" \
              "reviewed reset plan executed and emitted a durable receipt"
          else
            record "reset execution" "contract" "FAIL" "1" \
              "skipped because --dry-run did not remain non-destructive alongside --yes" \
              "$LAST_LIVE_LOG"
          fi
        else
          record "reset execution" "contract" "FAIL" "1" \
            "skipped because reset plan was not dry-run, SQLite-scoped, and reviewable" \
            "$reset_plan_log"
        fi
        export AXON_DATA_DIR="$saved_data_dir"
        ;;
      "prune plan")
        prune_fixture="$OUTDIR/prune-source"
        mkdir -p "$prune_fixture"
        printf '# Prune fixture\nGeneration one.\n' >"$prune_fixture/content.md"
        timeout "${TIMEOUT_SECS}s" "$AXON_BIN" source "$prune_fixture" --wait true \
          --collection "$AXON_COLLECTION" --json \
          >"$OUTDIR/logs/fixture-prune-source.json" \
          2>"$OUTDIR/logs/fixture-prune-source.stderr.log"
        prune_source_id="$(jq -r '.source_id // empty' "$OUTDIR/logs/fixture-prune-source.json")"
        prune_generation="$(jq -r '.generation // empty' "$OUTDIR/logs/fixture-prune-source.json")"
        printf '# Prune fixture\nGeneration two has changed content.\n' \
          >"$prune_fixture/content.md"
        timeout "${TIMEOUT_SECS}s" "$AXON_BIN" source "$prune_fixture" --wait true \
          --collection "$AXON_COLLECTION" --json \
          >"$OUTDIR/logs/fixture-prune-current-source.json" \
          2>"$OUTDIR/logs/fixture-prune-current-source.stderr.log"
        # shellcheck disable=SC2016
        assert_live_json "prune fixture advanced generation" \
          "$OUTDIR/logs/fixture-prune-current-source.json" \
          --arg old_generation "$prune_generation" \
          '.status == "completed" and .generation != $old_generation'
        run_live "$name" prune plan "$prune_source_id" --generation "$prune_generation" --json
        prune_plan_id="$(jq -r '.plan_id // .plan.plan_id // .plan.job_id // .plan.id // empty' "$LAST_LIVE_LOG" 2>/dev/null)"
        ;;
      "prune exec") run_live "$name" prune exec "$prune_plan_id" --generation "$prune_generation" --confirm --json ;;
      "preflight") run_live "$name" preflight --config --json ;;
      "smoke") run_live "$name" smoke --json ;;
      "compose up")
        run_live_setup_home "$name" compose up --json
        assert_live_json "compose up phases" "$LAST_LIVE_LOG" \
          '.has_errors == false and any(.phases[]; .name == "compose-up" and .status == "ok")'
        assert_live_container_stable "compose up container stays up" \
          "${isolated_compose_project}-axon"
        ;;
      "compose down")
        run_live_setup_home "$name" compose down --json
        assert_live_json "compose down phases" "$LAST_LIVE_LOG" \
          '.has_errors == false and any(.phases[]; .name == "compose-down" and .status == "ok")'
        ;;
      "compose restart")
        run_live_setup_home "compose restart fixture up" compose up --json
        assert_live_json "compose restart fixture up phases" "$LAST_LIVE_LOG" \
          '.has_errors == false and any(.phases[]; .name == "compose-up" and .status == "ok")'
        run_live_setup_home "$name" compose restart --json
        assert_live_json "compose restart phases" "$LAST_LIVE_LOG" \
          '.has_errors == false and any(.phases[]; .name == "compose-restart" and .status == "ok")'
        ;;
      "compose rebuild")
        run_live_setup_home "$name" compose rebuild --json
        assert_live_json "compose rebuild phases" "$LAST_LIVE_LOG" \
          '.has_errors == false
           and any(.phases[]; .name == "compose-build" and .status == "ok")
           and any(.phases[]; .name == "compose-up" and .status == "ok")'
        ;;
      "setup plugin-hook")
        run_live_setup_home "$name" setup plugin-hook --no-setup --json
        assert_live_json "setup plugin-hook policy" "$LAST_LIVE_LOG" \
          '.action != null and .exit_policy != null and .stack != null'
        cp "$LAST_LIVE_LOG" "$OUTDIR/logs/setup-plugin-hook-primary.json"
        run_live_setup_home "setup hook alias" setup hook --no-setup --json
        assert_live_json "setup hook alias policy" "$LAST_LIVE_LOG" \
          '.action != null and .exit_policy != null and .stack != null'
        if jq -S '{action,exit_policy,stack}' "$OUTDIR/logs/setup-plugin-hook-primary.json" \
          | cmp -s - <(jq -S '{action,exit_policy,stack}' "$LAST_LIVE_LOG"); then
          record "setup hook alias equivalence" "contract" "PASS" "0" \
            "setup hook == setup plugin-hook" "$LAST_LIVE_LOG"
          record_behavior_args "setup hook alias" "${PREPARED_ARGS[@]}"
        else
          failures=$((failures + 1))
          record "setup hook alias equivalence" "contract" "FAIL" "1" \
            "setup hook == setup plugin-hook" "$LAST_LIVE_LOG"
        fi
        ;;
      "setup init")
        run_live_setup_home "$name" setup init \
          --mcp-host 127.0.0.1 --mcp-port "$LIVE_SETUP_PORT" --auth-mode oauth \
          --mcp-token live-fixture-token \
          --oauth-public-url https://axon.invalid \
          --google-client-id live-fixture-google-client \
          --google-client-secret live-fixture-google-secret \
          --auth-admin-email live-fixture@example.invalid \
          --tavily-api-key live-fixture-tavily \
          --github-token live-fixture-github \
          --reddit-client-id live-fixture-reddit-client \
          --reddit-client-secret live-fixture-reddit-secret \
          --json
        assert_live_text "setup init OAuth URL persisted" "$SETUP_HOME/.axon/.env" \
          "AXON_PUBLIC_URL=https://axon.invalid"
        assert_live_text "setup init optional credentials persisted" "$SETUP_HOME/.axon/.env" \
          "REDDIT_CLIENT_ID=live-fixture-reddit-client"
        ;;
      "setup check") run_live_setup_check ;;
      "setup targets") run_live_setup_home "$name" setup targets --json ;;
      "setup install") run_live_setup_home "$name" setup install --json ;;
      "setup config rewrite") run_live_setup_home "$name" setup config rewrite --dry-run --json ;;
      "mcp") run_live_server "$name" "$LIVE_MCP_PORT" mcp --transport http ;;
      "migrate")
        migrate_from="${AXON_COLLECTION}_unnamed"
        migrate_to="${AXON_COLLECTION}_named"
        isolated_collections+=("$migrate_from" "$migrate_to")
        curl -fsS -X PUT "${QDRANT_URL%/}/collections/$migrate_from" -H 'content-type: application/json' \
          --data '{"vectors":{"size":1024,"distance":"Cosine"}}' >"$OUTDIR/logs/migrate-create.json"
        jq -nc '{points:[{id:1,vector:[range(1024)|0],payload:{chunk_text:"Axon live migration fixture"}}]}' \
          | curl -fsS -X PUT "${QDRANT_URL%/}/collections/$migrate_from/points?wait=true" -H 'content-type: application/json' --data-binary @- \
            >"$OUTDIR/logs/migrate-upsert.json"
        run_live "$name" migrate --from "$migrate_from" --to "$migrate_to" --json
        curl -fsS -X DELETE "${QDRANT_URL%/}/collections/$migrate_from" >"$OUTDIR/logs/migrate-delete-from.json"
        curl -fsS -X DELETE "${QDRANT_URL%/}/collections/$migrate_to" >"$OUTDIR/logs/migrate-delete-to.json"
        ;;
      "config list")
        timeout "${TIMEOUT_SECS}s" "$AXON_BIN" config set retrieval.hybrid-candidates 72 --json >"$OUTDIR/logs/fixture-config-set.json" 2>"$OUTDIR/logs/fixture-config-set.stderr.log"
        timeout "${TIMEOUT_SECS}s" "$AXON_BIN" config set AXON_LIVE_HARNESS_TOKEN live-fixture-value --env --json \
          >"$OUTDIR/logs/fixture-config-set-env.json"
        run_live "$name" config list --env --reveal --json
        assert_live_json "config list env reveal" "$LAST_LIVE_LOG" \
          '.env.AXON_LIVE_HARNESS_TOKEN == "live-fixture-value"'
        run_live "$name" config list --toml --reveal --json
        assert_live_json "config list TOML reveal" "$LAST_LIVE_LOG" \
          '.toml["retrieval.hybrid-candidates"] == "72"'
        ;;
      "config get")
        run_live "$name" config get AXON_LIVE_HARNESS_TOKEN --env --reveal --json
        assert_live_json "config get env reveal" "$LAST_LIVE_LOG" \
          '.present == true and .value == "live-fixture-value"'
        run_live "$name" config get retrieval.hybrid-candidates --toml --reveal --json
        assert_live_json "config get TOML reveal" "$LAST_LIVE_LOG" \
          '.present == true and .value == "72"'
        ;;
      "config set")
        run_live "$name" config set AXON_LIVE_HARNESS_SET live-fixture-set --env --json
        run_live "$name" config set retrieval.hybrid-candidates 73 --toml --json
        ;;
      "config unset")
        run_live "$name" config unset AXON_LIVE_HARNESS_SET --env --json
        run_live "$name" config unset retrieval.hybrid-candidates --toml --json
        ;;
      "config path")
        run_live "$name" config path --json
        # shellcheck disable=SC2016
        assert_live_json "config path uses isolated files" "$LAST_LIVE_LOG" \
          --arg toml "$AXON_CONFIG_PATH" --arg env "$AXON_ENV_FILE" \
          '.toml_path == $toml and .env_path == $env'
        prove_option_behavior "@global" "--json" "config path stdout parsed and matched isolated paths"
        ;;
      "sync pending") run_live "$name" sync pending --json ;;
      "update")
        run_live_setup_home "$name" update --repo dinglebear-ai/axon \
          --version v6.2.1 --force --no-container --json
        assert_live_json "update pinned Axon release" "$LAST_LIVE_LOG" \
          '.installed == true and .version == "6.2.1"'
        install -m 0755 "$HARNESS_SOURCE_BIN" "$AXON_BIN"
        ;;
      "palette")
        palette_display=":$((100 + $$ % 500))"
        Xvfb "$palette_display" -screen 0 1280x720x24 -nolisten tcp \
          >"$OUTDIR/logs/palette-xvfb.log" 2>&1 &
        palette_xvfb_pid=$!
        sleep 0.5
        DISPLAY="$palette_display" run_live_setup_home "$name" palette install --method pull --json
        DISPLAY="$palette_display" run_live_setup_home "palette launch" palette launch --json
        DISPLAY="$palette_display" run_live_setup_home "palette desktop" palette desktop --json
        DISPLAY="$palette_display" run_live_setup_home "palette autostart" palette autostart --json
        kill -TERM "$palette_xvfb_pid" 2>/dev/null || true
        wait "$palette_xvfb_pid" 2>/dev/null || true
        palette_source_sha256="$(sha256sum -- "$HARNESS_SOURCE_BIN" | awk '{print $1}')"
        palette_worktree_sha256="$(worktree_content_fingerprint)"
        if [ "$palette_source_sha256" = "$HARNESS_SOURCE_BIN_SHA256" ]; then
          record "palette source binary isolation" "contract" "PASS" "0" \
            "target/debug/axon checksum unchanged" "$HARNESS_SOURCE_BIN"
        else
          failures=$((failures + 1))
          record "palette source binary isolation" "contract" "FAIL" "1" \
            "target/debug/axon checksum changed" "$HARNESS_SOURCE_BIN"
        fi
        if [ "$palette_worktree_sha256" = "$WORKTREE_CONTENT_SHA256" ]; then
          record "palette worktree isolation" "contract" "PASS" "0" \
            "tracked and non-ignored source files unchanged" "$ROOT_DIR"
        else
          failures=$((failures + 1))
          record "palette worktree isolation" "contract" "FAIL" "1" \
            "source worktree content changed" "$ROOT_DIR"
        fi
        ;;
    *) return 1 ;;
  esac
  return 0
}
