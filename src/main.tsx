import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import "./styles/globals.css";
import "./styles/controls.css";
import "./styles/flow-bar.css";
import "./styles/writing-tools.css";
import "./styles/settings.css";
import "./styles/onboarding.css";

const root = document.getElementById("root");
if (!root) throw new Error("Kivo root element is missing");

createRoot(root).render(
  <StrictMode>
    <ErrorBoundary><App /></ErrorBoundary>
  </StrictMode>,
);
