// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { SessionViewer } from "./SessionViewer";

afterEach(cleanup);

it("renders partial hostile semantic events as bounded text", () => {
  const { container } = render(
    <SessionViewer
      events={[
        {
          position: 7,
          timestamp: "2026-08-28T20:00:00Z",
          kind: "assistant",
          text: '<img src=x onerror="alert(1)"> **not executable**',
          redacted: true,
          parse_warning: "partial transcript",
        },
      ]}
      scrollTop={0}
      onScroll={() => undefined}
    />,
  );
  expect(screen.getByText(/<img src=x onerror=/)).toBeInTheDocument();
  expect(screen.getByText("redacted")).toBeInTheDocument();
  expect(screen.getByText("Parse warning: partial transcript")).toBeInTheDocument();
  expect(container.querySelector("img")).toBeNull();
});
