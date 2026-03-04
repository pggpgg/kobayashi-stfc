import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

const STORAGE_KEY = 'kobayashi_workspace_mode';

export type WorkspaceMode = 'roster' | 'sandbox';

interface WorkspaceModeContextValue {
  mode: WorkspaceMode;
  setMode: (mode: WorkspaceMode) => void;
  ownedOnly: boolean;
}

const WorkspaceModeContext = createContext<WorkspaceModeContextValue | null>(null);

function loadStoredMode(): WorkspaceMode {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'roster' || stored === 'sandbox') return stored;
  } catch {}
  return 'roster';
}

export function WorkspaceModeProvider({ children }: { children: ReactNode }) {
  const [mode, setModeState] = useState<WorkspaceMode>(loadStoredMode);

  const setMode = useCallback((m: WorkspaceMode) => {
    setModeState(m);
    try {
      localStorage.setItem(STORAGE_KEY, m);
    } catch {}
  }, []);

  const value = useMemo(
    () => ({
      mode,
      setMode,
      ownedOnly: mode === 'roster',
    }),
    [mode, setMode],
  );

  return (
    <WorkspaceModeContext.Provider value={value}>
      {children}
    </WorkspaceModeContext.Provider>
  );
}

export function useWorkspaceMode() {
  const ctx = useContext(WorkspaceModeContext);
  if (!ctx) {
    throw new Error('useWorkspaceMode must be used within WorkspaceModeProvider');
  }
  return ctx;
}
