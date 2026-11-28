import React from "react";
import "@fontsource-variable/noto-serif-sc";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { AppearanceProvider } from "./appearance";
import "./styles.css";
import "./controls.css";
import "./collections.css";
import "./runtime.css";
import "./tour.css";

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <AppearanceProvider>
      <App />
    </AppearanceProvider>
  </React.StrictMode>,
);
