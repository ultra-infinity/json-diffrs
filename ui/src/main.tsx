import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./monaco";
// Monaco's icon font, also used by the explorer and the assistant.
import "monaco-editor/features/codicon/register";
import "./styles.css";
import { App } from "./App";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
