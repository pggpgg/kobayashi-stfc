import { styles } from "../lib/rosterProfileStyles";

/** Mod sync older than this is shown in red (stale). */
const MOD_SYNC_STALE_AFTER_MS = 24 * 60 * 60 * 1000;

export default function RosterProfileModSyncBanner({
  modSyncUtc,
  modSyncError,
}: {
  modSyncUtc: string | null | undefined;
  modSyncError: string | null;
}) {
  return (
    <div
      role="status"
      aria-live="polite"
      style={{
        marginBottom: "0.75rem",
        padding: "0.5rem 0.75rem",
        fontSize: "0.9rem",
        fontWeight: 500,
        background: "var(--surface)",
        border: "1px solid var(--border)",
        borderRadius: 8,
      }}
    >
      {modSyncError ? (
        <span style={{ color: "var(--error)" }}>{modSyncError}</span>
      ) : modSyncUtc === undefined ? (
        <span style={styles.muted}>Checking community mod sync…</span>
      ) : modSyncUtc === null ? (
        <span style={styles.muted}>
          No community mod sync recorded yet for this profile. Use the STFC
          Community Mod in-game to push roster, buildings, research, and other
          data to Kobayashi.
        </span>
      ) : (
        (() => {
          const t = Date.parse(modSyncUtc);
          const ok =
            !Number.isNaN(t) &&
            Date.now() - t >= 0 &&
            Date.now() - t < MOD_SYNC_STALE_AFTER_MS;
          const when = Number.isNaN(t)
            ? modSyncUtc
            : new Date(t).toLocaleString(undefined, {
                dateStyle: "short",
                timeStyle: "medium",
              });
          return (
            <span
              style={{
                color: ok ? "var(--success)" : "var(--error)",
              }}
            >
              Last community mod sync received: {when}
              {!Number.isNaN(t) && !ok ? " (stale)" : ""}
            </span>
          );
        })()
      )}
    </div>
  );
}
