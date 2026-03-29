import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { ProfileProvider } from './contexts/ProfileContext';
import { WorkspaceModeProvider } from './contexts/WorkspaceModeContext';
import App from './App';

vi.mock('./pages/Workspace', () => ({
  default: () => <div>WorkspacePageMarker</div>,
}));
vi.mock('./pages/ResultsLibrary', () => ({
  default: () => <div>ResultsLibraryPageMarker</div>,
}));
vi.mock('./pages/RosterProfile', () => ({
  default: () => <div>RosterProfilePageMarker</div>,
}));
vi.mock('./pages/DataMechanics', () => ({
  default: () => <div>DataMechanicsPageMarker</div>,
}));

vi.mock('./lib/api', () => ({
  fetchProfiles: vi.fn().mockResolvedValue({
    profiles: [{ id: 'p1', name: 'Main', sync_token: 't' }],
    default_id: 'p1',
  }),
}));

function renderApp(initialPath = '/') {
  return render(
    <MemoryRouter initialEntries={[initialPath]}>
      <WorkspaceModeProvider>
        <ProfileProvider>
          <App />
        </ProfileProvider>
      </WorkspaceModeProvider>
    </MemoryRouter>,
  );
}

describe('App navigation', () => {
  it('renders workspace on /', () => {
    renderApp('/');
    expect(screen.getByText('WorkspacePageMarker')).toBeTruthy();
  });

  it('navigates to Results Library, Roster, and Data via shell links', () => {
    renderApp('/');

    fireEvent.click(screen.getByRole('link', { name: 'Results Library' }));
    expect(screen.getByText('ResultsLibraryPageMarker')).toBeTruthy();

    fireEvent.click(screen.getByRole('link', { name: 'Roster & Profile' }));
    expect(screen.getByText('RosterProfilePageMarker')).toBeTruthy();

    fireEvent.click(screen.getByRole('link', { name: 'Data & Mechanics' }));
    expect(screen.getByText('DataMechanicsPageMarker')).toBeTruthy();

    fireEvent.click(screen.getByRole('link', { name: 'Workspace' }));
    expect(screen.getByText('WorkspacePageMarker')).toBeTruthy();
  });
});
