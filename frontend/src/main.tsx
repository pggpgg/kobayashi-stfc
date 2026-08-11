import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import ErrorBoundary from "./components/ErrorBoundary";
import { ProfileProvider } from "./contexts/ProfileContext";
import { WorkspaceModeProvider } from "./contexts/WorkspaceModeContext";
import "./index.css";

const rootEl = document.getElementById("root");
if (rootEl == null) {
  throw new Error("Missing element #root");
}

ReactDOM.createRoot(rootEl).render(
  <React.StrictMode>
    {/* The v6 `future` opt-ins (v7_startTransition, v7_relativeSplatPath) are the
        default behaviour in react-router v7, and the prop no longer exists. */}
    <BrowserRouter>
      <ErrorBoundary>
        <ProfileProvider>
          <WorkspaceModeProvider>
            <App />
          </WorkspaceModeProvider>
        </ProfileProvider>
      </ErrorBoundary>
    </BrowserRouter>
  </React.StrictMode>,
);
