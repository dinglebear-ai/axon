import React from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { AskStreamTransitionFixture } from "./components/palette/AskStreamTransitionFixture";
import { OperationResultFixture } from "./components/palette/OperationResultFixture";
import "./fonts.css";
import "./styles.css";

const fixture = new URLSearchParams(window.location.search).get("fixture");
const Root =
  fixture === "operation-results"
    ? OperationResultFixture
    : fixture === "ask-stream-transition"
      ? AskStreamTransitionFixture
      : App;

if (fixture) {
  document.body.dataset.fixture = fixture;
}

const rootElement = document.getElementById("app");
if (!rootElement) throw new Error("Missing #app root element");

createRoot(rootElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
