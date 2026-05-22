import { lazy, Suspense } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import Shell from "./components/Shell";
import Workspace from "./pages/Workspace";

const ResultsLibrary = lazy(() => import("./pages/ResultsLibrary"));
const RosterProfile = lazy(() => import("./pages/RosterProfile"));
const DataMechanics = lazy(() => import("./pages/DataMechanics"));
const PvpWorkspace = lazy(() => import("./pages/PvpWorkspace"));
const Sensitivity = lazy(() => import("./pages/Sensitivity"));

function RouteFallback() {
  return (
    <div
      style={{
        padding: "2rem 1.5rem",
        color: "var(--text-muted)",
        fontSize: "0.95rem",
      }}
    >
      Loading…
    </div>
  );
}

export default function App() {
  return (
    <Shell>
      <Suspense fallback={<RouteFallback />}>
        <Routes>
          <Route path="/" element={<Workspace />} />
          <Route path="/pvp" element={<PvpWorkspace />} />
          <Route path="/sensitivity" element={<Sensitivity />} />
          <Route path="/results" element={<ResultsLibrary />} />
          <Route path="/roster" element={<RosterProfile />} />
          <Route path="/data" element={<DataMechanics />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </Suspense>
    </Shell>
  );
}
