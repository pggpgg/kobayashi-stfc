import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { fetchProfiles } from '../lib/api';
import type { ProfileEntry } from '../lib/api';

const STORAGE_KEY = 'kobayashi_active_profile';

interface ProfileContextValue {
  activeProfileId: string | null;
  setActiveProfileId: (id: string) => void;
  profiles: ProfileEntry[];
  refreshProfiles: () => Promise<void>;
}

const ProfileContext = createContext<ProfileContextValue | null>(null);

export function ProfileProvider({ children }: { children: ReactNode }) {
  const [profiles, setProfiles] = useState<ProfileEntry[]>([]);
  const [activeProfileId, setActiveProfileIdState] = useState<string | null>(() => {
    try {
      return localStorage.getItem(STORAGE_KEY);
    } catch {
      return null;
    }
  });

  const refreshProfiles = useCallback(async () => {
    const data = await fetchProfiles();
    const list = data.profiles ?? [];
    setProfiles(list);
    const defaultId = data.default_id ?? list[0]?.id;
    setActiveProfileIdState((prev) => {
      if (prev && list.some((p) => p.id === prev)) return prev;
      if (defaultId) {
        try {
          localStorage.setItem(STORAGE_KEY, defaultId);
        } catch {}
        return defaultId;
      }
      return prev;
    });
  }, []);

  useEffect(() => {
    refreshProfiles();
  }, []);

  const setActiveProfileId = useCallback((id: string) => {
    setActiveProfileIdState(id);
    try {
      localStorage.setItem(STORAGE_KEY, id);
    } catch {}
  }, []);

  const value = useMemo(
    () => ({
      activeProfileId,
      setActiveProfileId,
      profiles,
      refreshProfiles,
    }),
    [activeProfileId, setActiveProfileId, profiles, refreshProfiles],
  );

  return <ProfileContext.Provider value={value}>{children}</ProfileContext.Provider>;
}

export function useProfile() {
  const ctx = useContext(ProfileContext);
  if (!ctx) {
    throw new Error('useProfile must be used within ProfileProvider');
  }
  return ctx;
}
