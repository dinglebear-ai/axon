import asyncio
import json
from pathlib import Path

from playwright.async_api import async_playwright, expect


BASE_URL = "http://127.0.0.1:1420/?fixture=ask-stream-transition"
OUT_DIR = Path("/tmp")


async def inspect_state(page, state: str):
    await page.goto(f"{BASE_URL}&state={state}", wait_until="commit", timeout=5000)
    await expect(page.get_by_role("group", name="Ask conversation")).to_be_visible(timeout=10_000)
    await page.screenshot(path=str(OUT_DIR / f"axon-palette-ask-{state}.png"), full_page=False)
    return await page.evaluate(
        """
        () => {
          const thread = document.querySelector(".ask-thread");
          const messages = [...document.querySelectorAll(".ask-message")].map((el) => el.textContent?.trim());
          return {
            state: new URLSearchParams(location.search).get("state"),
            threadClass: thread?.className ?? "",
            messageCount: messages.length,
            messages,
            hasStandaloneComplete: Boolean(document.querySelector(".output-status")),
            hasStandaloneCopy: Boolean(document.querySelector('[aria-label="Copy output"]')),
            hasSources: Boolean(document.querySelector(".ask-sources")),
            text: document.body.innerText,
          };
        }
        """
    )


async def main():
    async with async_playwright() as p:
        browser = await p.chromium.launch(headless=True)
        page = await browser.new_page(viewport={"width": 1280, "height": 720})
        messages = []
        page.on("console", lambda msg: messages.append({"type": msg.type, "text": msg.text}))
        page.on("pageerror", lambda err: messages.append({"type": "pageerror", "text": str(err)}))

        streaming = await inspect_state(page, "streaming")
        complete = await inspect_state(page, "complete")
        await browser.close()

    relevant = [msg for msg in messages if msg["type"] in ("error", "warning", "pageerror")]
    result = {"streaming": streaming, "complete": complete, "relevantMessages": relevant}
    print(json.dumps(result, indent=2))

    assert streaming["messageCount"] == 2
    assert complete["messageCount"] == 2
    assert "ask-thread-reader" not in complete["threadClass"]
    assert complete["hasStandaloneComplete"] is False
    assert complete["hasStandaloneCopy"] is False
    assert complete["hasSources"] is True
    assert relevant == []


asyncio.run(main())
