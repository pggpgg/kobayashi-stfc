import { Routes, Route, Navigate } from 'react-router-dom';
import Shell from './components/Shell';
import Workspace from './pages/Workspace';
import ResultsLibrary from './pages/ResultsLibrary';
import RosterProfile from './pages/RosterProfile';
import DataMechanics from './pages/DataMechanics';

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
