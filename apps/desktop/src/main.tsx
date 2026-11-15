import React from "react";
import ReactDOM from "react-dom/client";
import { tauriApi } from "./api";
import { App } from "./App";
import { StoreProvider } from "./context";
import { createAppStore } from "./store";
import "./styles.css";

const store = createAppStore(tauriApi);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <StoreProvider store={store}>
      <App />
    </StoreProvider>
  </React.StrictMode>,
);
