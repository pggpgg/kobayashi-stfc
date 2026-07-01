import { styles } from "../lib/rosterProfileStyles";

const BONUS_KEYS = ["weapon", "shield", "mitigation", "hull"] as const;

export default function RosterProfileBonusesTab({
  bonuses,
  onBonusChange,
  onSave,
  profileDirty,
  profileError,
}: {
  bonuses: Record<string, number>;
  onBonusChange: (key: string, value: number) => void;
  onSave: () => void;
  profileDirty: boolean;
  profileError: string | null;
}) {
  return (
    <section
      style={{
        padding: "1rem",
        background: "var(--surface)",
        border: "1px solid var(--border)",
        borderRadius: 8,
      }}
    >
      <p
        style={{
          margin: "0 0 0.75rem",
          fontSize: "0.9rem",
          color: "var(--text-muted)",
        }}
      >
        Quick mode: enter effective bonus percentages (e.g. weapon, shield,
        mitigation).
      </p>
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 8,
          maxWidth: 400,
        }}
      >
        {BONUS_KEYS.map((key) => (
          <label key={key} style={styles.rowCenter}>
            <span style={{ width: 100 }}>{key} %</span>
            <input
              type="number"
              step={0.1}
              value={bonuses[key] ?? ""}
              onChange={(e) => onBonusChange(key, Number(e.target.value) || 0)}
              style={{
                padding: "0.4rem",
                background: "var(--bg)",
                border: "1px solid var(--border)",
                borderRadius: 4,
                color: "var(--text)",
              }}
            />
          </label>
        ))}
      </div>
      <button
        type="button"
        onClick={onSave}
        disabled={!profileDirty}
        style={{
          marginTop: 12,
          padding: "0.5rem 1rem",
          background: profileDirty ? "var(--accent)" : "var(--border)",
          border: "none",
          borderRadius: 6,
          color: "var(--bg)",
        }}
      >
        Save profile
      </button>
      {profileError && <div style={styles.errorNote}>{profileError}</div>}
    </section>
  );
}
