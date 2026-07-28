import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./i18n";
import { App } from "./app/App";
import "./styles/globals.css";

const rootElement = document.getElementById("root");

if (!rootElement) {
  throw new Error("Missing #root mount point");
}

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
