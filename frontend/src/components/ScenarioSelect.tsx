import { ENEMY_TYPE_OPTIONS } from "../lib/enemyTypes";

interface ScenarioSelectProps {
  /** Selected enemy_type id, or "" for Auto (server infers from the target). */
  value: string;
  onChange: (id: string) => void;
}

/**
 * Combat-scenario picker. Drives officer eligibility filtering during optimization and the live
 * eligibility badges in the crew builder. "Auto" leaves the scenario unset so the server infers it
 * from the target (PvP / group armada / outpost / generic hostile).
 */
export function ScenarioSelect({ value, onChange }: ScenarioSelectProps) {
  return (
    <label style={{ display: "flex", flexDirection: "column", gap: "0.2rem" }}>
      <span style={{ fontSize: "0.7rem", color: "var(--text-muted)" }}>
        Scenario
      </span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        title="Combat scenario — drives officer eligibility filtering and crew badges"
        style={{
          background: "var(--surface, #11202b)",
          color: "var(--text, #cfe3ee)",
          border: "1px solid var(--border, #2a4a5c)",
          borderRadius: 4,
          padding: "0.3rem 0.4rem",
          fontSize: "0.85rem",
        }}
      >
        <option value="">⟳ Auto (from target)</option>
        {ENEMY_TYPE_OPTIONS.map((o) => (
          <option key={o.id} value={o.id}>
            {o.icon} {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}
