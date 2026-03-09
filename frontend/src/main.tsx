import React from 'react';
import ReactDOM from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import { ProfileProvider } from './contexts/ProfileContext';
import { WorkspaceModeProvider } from './contexts/WorkspaceModeContext';
import App from './App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <BrowserRouter>
      <ProfileProvider>
        <WorkspaceModeProvider>
          <App />
        </WorkspaceModeProvider>
      </ProfileProvider>
    </BrowserRouter>
  </React.StrictMode>
);
