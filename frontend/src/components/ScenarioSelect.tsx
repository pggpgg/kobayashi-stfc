import {
  ENEMY_TYPE_OPTIONS,
  ENEMY_TYPE_OVERRIDE_OPTIONS,
  enemyTypeLabel,
} from "../lib/enemyTypes";

interface ScenarioSelectProps {
  /** Selected enemy_type override id, or "" for Auto (server infers from the target). */
  value: string;
  onChange: (id: string) => void;
  /** Auto-inferred base enemy_type for the currently selected hostile (e.g. "solo_armadas"). */
  inferredEnemyType?: string | null;
}

/**
 * Combat-scenario override picker. Drives officer eligibility filtering and combat mode override.
 * "Auto" leaves the scenario unset so the server infers it from the target (PvP / solo armada /
 * group armada / outpost / outpost retaliator / Q's Trial / mission boss / generic hostile).
 */
export function ScenarioSelect({
  value,
  onChange,
  inferredEnemyType = null,
}: ScenarioSelectProps) {
  const autoLabel = inferredEnemyType
    ? `⟳ Auto (${enemyTypeLabel(inferredEnemyType)})`
    : "⟳ Auto (from target)";
  // Keep `"waves"` selectable if a caller/state already holds it, otherwise prefer the dedicated toggle.
  const options =
    value === "waves" ? ENEMY_TYPE_OPTIONS : ENEMY_TYPE_OVERRIDE_OPTIONS;
  return (
    <label style={{ display: "flex", flexDirection: "column", gap: "0.2rem" }}>
      <span style={{ fontSize: "0.7rem", color: "var(--text-muted)" }}>
        Mode
      </span>
      <select
        aria-label="Combat mode override"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        title="Combat engagement mode — Auto infers from the selected target; override only when needed"
        style={{
          background: "var(--surface, #11202b)",
          color: "var(--text, #cfe3ee)",
          border: "1px solid var(--border, #2a4a5c)",
          borderRadius: 4,
          padding: "0.3rem 0.4rem",
          fontSize: "0.85rem",
        }}
      >
        <option value="">{autoLabel}</option>
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.icon} {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}
