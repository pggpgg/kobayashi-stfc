/** Session-only persistence so a page refresh can reattach to an in-flight async optimize job. */

const STORAGE_KEY = "kobayashi_active_optimize_job_v1";

export interface PersistedOptimizeJob {
  jobId: string;
  profileId: string;
}

export function persistOptimizeJob(
  jobId: string,
  profileId: string | null,
): void {
  try {
    sessionStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ jobId, profileId: profileId ?? "" }),
    );
  } catch {
    /* quota / private mode */
  }
}

export function clearPersistedOptimizeJob(): void {
  try {
    sessionStorage.removeItem(STORAGE_KEY);
  } catch {
    /* ignore */
  }
}

export function readPersistedOptimizeJob(): PersistedOptimizeJob | null {
  try {
    const raw = sessionStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const o = JSON.parse(raw) as { jobId?: unknown; profileId?: unknown };
    if (typeof o.jobId !== "string" || !o.jobId.trim()) return null;
    return {
      jobId: o.jobId.trim(),
      profileId: typeof o.profileId === "string" ? o.profileId : "",
    };
  } catch {
    return null;
  }
}

export function profileMatchesPersisted(
  activeProfileId: string | null,
  persistedProfileId: string,
): boolean {
  return (activeProfileId ?? "") === persistedProfileId;
}
