#!/usr/bin/env python3
"""Drive the unpacked Axon extension in a real Chrome over CDP.

Secrets are accepted only through environment variables and are never logged or
written into evidence. The Chrome profile and extension must already be running.
"""

import argparse
import json
import os
from pathlib import Path

from playwright.sync_api import sync_playwright


def attach(cdp: str):
    playwright = sync_playwright().start()
    browser = playwright.chromium.connect_over_cdp(cdp)
    context = browser.contexts[0]
    return playwright, browser, context


def page_for(context, url: str):
    page = context.new_page()
    errors = []
    failed = []
    page.on("console", lambda message: errors.append(f"console {message.type}: {message.text}") if message.type == "error" else None)
    page.on("pageerror", lambda error: errors.append(f"pageerror: {error}"))
    page.on("requestfailed", lambda request: failed.append(f"{request.method} {request.url}: {request.failure}"))
    page.goto(url, wait_until="domcontentloaded")
    return page, errors, failed


def configure(context, extension_id: str, server: str, token: str, evidence: Path):
    page, errors, failed = page_for(context, f"chrome-extension://{extension_id}/src/options/options.html")
    page.get_by_label("Axon server").fill(server)
    page.get_by_label("Token").fill(token)
    page.screenshot(path=evidence / "cp01_options_filled.png", full_page=True)
    page.get_by_role("button", name="Save settings").click()
    page.wait_for_timeout(5000)
    page.screenshot(path=evidence / "cp02_options_saved.png", full_page=True)
    return page, errors, failed


def check_api(page, evidence: Path):
    page.get_by_role("button", name="Check API").click()
    page.get_by_text("Online", exact=True).wait_for(timeout=20000)
    page.screenshot(path=evidence / "cp03_api_online.png", full_page=True)


def wait_result(page, timeout=90000):
    page.locator(".ext-loading").wait_for(state="hidden", timeout=timeout)
    badge = page.locator(".ext-result-head .ext-statusbadge")
    badge.wait_for(timeout=timeout)
    return badge.inner_text().strip()


def require_http_200(page, timeout=90000):
    status = wait_result(page, timeout)
    if status != "200":
        raise AssertionError(f"expected HTTP 200, got {status}: {page.locator('body').inner_text()[:1000]}")


def back_to_browse(page):
    page.locator('.ext-result-head button[aria-label="Back"]').click()
    page.get_by_text("THIS PAGE", exact=True).wait_for(timeout=10000)


def run_panel(context, extension_id: str, evidence: Path):
    page, errors, failed = page_for(context, f"chrome-extension://{extension_id}/src/sidepanel/sidepanel.html")
    page.get_by_text("THIS PAGE", exact=True).wait_for(timeout=10000)
    page.wait_for_timeout(2000)
    page.screenshot(path=evidence / "cp04_sidepanel_browse.png", full_page=True)

    page.locator("button.ext-row").filter(has_text="Status").click()
    require_http_200(page)
    page.screenshot(path=evidence / "cp05_status.png", full_page=True)
    back_to_browse(page)

    page.locator("button.ext-row").filter(has_text="Query").click()
    page.locator("input.ext-arginput").fill("Axon pipeline")
    page.get_by_role("button", name="Run").click()
    require_http_200(page)
    page.screenshot(path=evidence / "cp06_query.png", full_page=True)
    back_to_browse(page)

    page.locator("button.ext-row").filter(has_text="Ask").click()
    page.locator("input.ext-arginput").fill("What is Axon?")
    page.get_by_role("button", name="Run").click()
    require_http_200(page, timeout=120000)
    page.screenshot(path=evidence / "cp07_ask.png", full_page=True)
    back_to_browse(page)

    page.get_by_role("button", name="Scrape", exact=True).click()
    wait_result(page, timeout=120000)
    page.screenshot(path=evidence / "cp08_scrape_current_page.png", full_page=True)
    return page, errors, failed


def verify_persistence(context, extension_id: str, server: str, evidence: Path):
    page, errors, failed = page_for(context, f"chrome-extension://{extension_id}/src/options/options.html")
    assert page.get_by_label("Axon server").input_value() == server
    assert page.get_by_label("Token").input_value(), "stored bearer token was empty after restart"
    page.screenshot(path=evidence / "cp09_restart_persistence.png", full_page=True)
    return page, errors, failed


def run_ask(context, extension_id: str, evidence: Path):
    page, errors, failed = page_for(context, f"chrome-extension://{extension_id}/src/sidepanel/sidepanel.html")
    page.locator("button.ext-row").filter(has_text="Ask").click()
    page.locator("input.ext-arginput").fill("What is Axon?")
    page.get_by_role("button", name="Run").click()
    require_http_200(page, timeout=120000)
    page.screenshot(path=evidence / "cp10_ask_citations_fixed.png", full_page=True)
    return page, errors, failed


def run_blocked_scheme(context, extension_id: str, evidence: Path):
    page, errors, failed = page_for(context, f"chrome-extension://{extension_id}/src/sidepanel/sidepanel.html")
    page.locator("button.ext-row").filter(has_text="Scrape").click()
    page.locator("input.ext-arginput").fill("chrome://extensions")
    page.get_by_role("button", name="Run").click()
    page.get_by_text("Axon can't capture chrome:", exact=False).wait_for(timeout=10000)
    page.screenshot(path=evidence / "cp11_blocked_scheme.png", full_page=True)
    return page, errors, failed


def run_popup(context, extension_id: str, evidence: Path):
    page, errors, failed = page_for(context, f"chrome-extension://{extension_id}/src/popup/popup.html")
    page.get_by_text("Conversation", exact=True).wait_for(timeout=10000)
    page.screenshot(path=evidence / "cp12_popup.png", full_page=True)
    page.locator("#command-input").fill("status")
    page.locator("#command-send").click()
    page.get_by_text("RECENT JOBS", exact=False).wait_for(timeout=30000)
    page.screenshot(path=evidence / "cp13_popup_status.png", full_page=True)
    return page, errors, failed


def run_redaction(context, extension_id: str, evidence: Path):
    page, errors, failed = page_for(context, f"chrome-extension://{extension_id}/src/sidepanel/sidepanel.html")
    secret = "axon_test_secret_value_1234567890"
    bodies = []
    page.on("request", lambda request: bodies.append(request.post_data or "") if request.url.endswith("/v1/query") else None)
    page.locator("button.ext-row").filter(has_text="Query").click()
    page.locator("input.ext-arginput").fill(f"Authorization: Bearer {secret}")
    page.get_by_role("button", name="Run").click()
    require_http_200(page)
    assert bodies, "no live /v1/query request was observed"
    assert all(secret not in body for body in bodies), "secret reached the live request body"
    assert any("[REDACTED]" in body for body in bodies), "redaction marker was absent from request body"
    (evidence / "redaction.json").write_text(json.dumps({"request_count": len(bodies), "secret_present": False, "redaction_marker_present": True}, indent=2), encoding="utf-8")
    page.screenshot(path=evidence / "cp14_query_redacted.png", full_page=True)
    return page, errors, failed


def run_scrape(context, extension_id: str, evidence: Path):
    page, errors, failed = page_for(context, f"chrome-extension://{extension_id}/src/sidepanel/sidepanel.html")
    # The harness renders the side panel in a normal extension tab. Ensure the
    # browser's active tab is still a capturable web page before invoking the
    # command, matching native side-panel use.
    worker = next(worker for worker in context.service_workers if worker.url.startswith(f"chrome-extension://{extension_id}/"))
    activated = worker.evaluate(
        """async () => {
          const tabs = await chrome.tabs.query({});
          let tab = tabs.find((candidate) => /^https?:/.test(candidate.url || ''));
          if (!tab?.id) tab = await chrome.tabs.create({url: 'https://example.com/', active: false});
          if (!tab?.id) return null;
          await chrome.tabs.update(tab.id, {active: true});
          return String(tab.id);
        }"""
    )
    if not activated:
        raise AssertionError("no active http(s) fixture tab was available for current-page scrape")
    page.get_by_role("button", name="Scrape", exact=True).click()
    status = wait_result(page, timeout=180000)
    page.screenshot(path=evidence / "cp16_scrape_repaired_service.png", full_page=True)
    (evidence / "scrape-rerun.json").write_text(
        json.dumps({"http_status": status, "body": page.locator("body").inner_text()[:4000]}, indent=2),
        encoding="utf-8",
    )
    if status != "200":
        raise AssertionError(f"current-page scrape returned HTTP {status}")
    return page, errors, failed


def run_shortcut(context, extension_id: str, evidence: Path):
    page, errors, failed = page_for(context, "chrome://extensions/shortcuts")
    shortcut = page.locator(f'input[aria-label*="Activate the extension for Axon"]')
    expected = os.environ.get("AXON_E2E_SHORTCUT", "Ctrl + Shift + 9")
    if shortcut.input_value() != expected:
        raise AssertionError(f"Axon shortcut is {shortcut.input_value()!r}, expected {expected!r}")
    page.screenshot(path=evidence / "cp19_shortcut_assigned.png", full_page=True)
    return page, errors, failed


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("phase", choices=["configure", "check", "panel", "persistence", "ask", "blocked", "popup", "redaction", "scrape", "shortcut"])
    parser.add_argument("--cdp", default=os.environ.get("CHROME_CDP", "http://127.0.0.1:9223"))
    parser.add_argument("--extension-id", required=True)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    args.evidence.mkdir(parents=True, exist_ok=True)

    playwright, browser, context = attach(args.cdp)
    try:
        server = os.environ.get("AXON_E2E_URL", "")
        token = os.environ.get("AXON_E2E_TOKEN", "")
        if not server or not token:
            raise SystemExit("AXON_E2E_URL and AXON_E2E_TOKEN are required")
        if args.phase == "panel":
            page, errors, failed = run_panel(context, args.extension_id, args.evidence)
        elif args.phase == "ask":
            page, errors, failed = run_ask(context, args.extension_id, args.evidence)
        elif args.phase == "blocked":
            page, errors, failed = run_blocked_scheme(context, args.extension_id, args.evidence)
        elif args.phase == "popup":
            page, errors, failed = run_popup(context, args.extension_id, args.evidence)
        elif args.phase == "redaction":
            page, errors, failed = run_redaction(context, args.extension_id, args.evidence)
        elif args.phase == "scrape":
            page, errors, failed = run_scrape(context, args.extension_id, args.evidence)
        elif args.phase == "shortcut":
            page, errors, failed = run_shortcut(context, args.extension_id, args.evidence)
        elif args.phase == "persistence":
            page, errors, failed = verify_persistence(context, args.extension_id, server, args.evidence)
        else:
            page, errors, failed = configure(context, args.extension_id, server, token, args.evidence)
            if args.phase == "check":
                check_api(page, args.evidence)
        (args.evidence / "browser-errors.json").write_text(
            json.dumps({"errors": errors, "request_failed": failed}, indent=2), encoding="utf-8"
        )
        print(json.dumps({"phase": args.phase, "url": page.url, "errors": len(errors), "request_failed": len(failed)}))
    finally:
        # Stopping Playwright disconnects from a CDP-attached browser. Do not
        # call browser.close(), which would terminate the user's test Chrome.
        playwright.stop()


if __name__ == "__main__":
    main()
