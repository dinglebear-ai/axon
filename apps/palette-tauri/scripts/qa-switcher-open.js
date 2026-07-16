async () => {
  const start = await codemode.nextjs_devtools.browser_eval({ action: "start" });
  const navigate = await codemode.nextjs_devtools.browser_eval({
    action: "navigate",
    url: "http://127.0.0.1:1420/",
  });
  const interact = await codemode.nextjs_devtools.browser_eval({
    action: "evaluate",
    script: `
      async () => {
        const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
        const input = document.querySelector('[aria-label="Axon command"]');
        if (!input) throw new Error("missing command input");
        input.focus();
        input.value = "scr";
        input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: "scr" }));
        input.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Enter" }));
        await sleep(80);
        const trigger = document.querySelector('[aria-label^="Switch from Scrape URL"]');
        if (!trigger) throw new Error("missing switcher trigger");
        trigger.click();
        await sleep(80);
        const wrap = document.querySelector(".command-input-wrap");
        const shell = document.querySelector(".command-bar");
        const menu = document.querySelector(".command-action-menu");
        if (!wrap || !shell || !menu) throw new Error("missing switcher menu");
        const wrapRect = wrap.getBoundingClientRect();
        const shellRect = shell.getBoundingClientRect();
        const menuRect = menu.getBoundingClientRect();
        return {
          title: document.title,
          menuVisible: Boolean(menu),
          hasAsk: document.body.innerText.includes("Ask"),
          hasPrune: document.body.innerText.includes("Prune"),
          shell: { x: shellRect.x, y: shellRect.y, width: shellRect.width, height: shellRect.height },
          wrap: { x: wrapRect.x, y: wrapRect.y, width: wrapRect.width, height: wrapRect.height },
          menu: { x: menuRect.x, y: menuRect.y, width: menuRect.width, height: menuRect.height },
          widthDelta: Math.abs(menuRect.width - shellRect.width),
          leftDelta: Math.abs(menuRect.x - shellRect.x),
          verticalGap: Math.round((menuRect.y - shellRect.bottom) * 100) / 100,
          attached: Math.abs(menuRect.y - shellRect.bottom) <= 2,
          clippedTop: menuRect.y < 0,
          clippedBottom: menuRect.bottom > window.innerHeight,
          clippedLeft: menuRect.x < 0,
          clippedRight: menuRect.right > window.innerWidth,
          focusBoxShadow: getComputedStyle(wrap).boxShadow,
          viewport: { width: window.innerWidth, height: window.innerHeight },
        };
      }
    `,
  });
  const consoleMessages = await codemode.nextjs_devtools.browser_eval({
    action: "console_messages",
  });
  const screenshot = await codemode.nextjs_devtools.browser_eval({
    action: "screenshot",
  });
  return { start, navigate, interact, consoleMessages, screenshot };
}
