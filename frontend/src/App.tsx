import { Navigate, Route, Routes } from "react-router-dom";
import Shell from "./components/Shell";
import DataMechanics from "./pages/DataMechanics";
import ResultsLibrary from "./pages/ResultsLibrary";
import RosterProfile from "./pages/RosterProfile";
import Workspace from "./pages/Workspace";

export default function App() {
  return (
    <Shell>
      <Routes>
        <Route path="/" element={<Workspace />} />
        <Route path="/results" element={<ResultsLibrary />} />
        <Route path="/roster" element={<RosterProfile />} />
        <Route path="/data" element={<DataMechanics />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Shell>
  );
}
