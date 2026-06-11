import React from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { OperationResultFixture } from "./components/palette/OperationResultFixture";
import "./fonts.css";
import "./styles.css";

const Root = new URLSearchParams(window.location.search).get("fixture") === "operation-results" ? OperationResultFixture : App;

createRoot(document.getElementById("app")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
