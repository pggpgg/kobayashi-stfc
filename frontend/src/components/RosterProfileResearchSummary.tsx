import type {
  ResearchCombatSummary,
  ResearchConditionalBonusLine,
} from "../lib/api";
import {
  formatProfileCombatBonusDelta,
  formatProfileCombatBonusEntry,
  formatProfileCombatBonusListValue,
} from "../lib/profileCombatBonusDisplay";
import { styles } from "../lib/rosterProfileStyles";

function formatResearchBonusMap(m?: Record<string, number>): string {
  if (!m || Object.keys(m).length === 0) return "—";
  return Object.entries(m)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([k, v]) => formatProfileCombatBonusEntry(k, v))
    .join("; ");
}

function formatOwnerFactionResearch(
  m?: Record<string, Record<string, number>>,
): string {
  if (!m || Object.keys(m).length === 0) return "—";
  return Object.entries(m)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([faction, inner]) => {
      const stats = Object.entries(inner)
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([k, v]) => formatProfileCombatBonusEntry(k, v))
        .join(", ");
      return `${faction}: ${stats}`;
    })
    .join("; ");
}

function formatConditionalResearch(
  lines?: ResearchConditionalBonusLine[],
): string {
  if (!lines || lines.length === 0) return "—";
  return lines
    .map((line) => {
      const gate = line.condition_label ?? "conditional";
      const runtime = line.requires_runtime_state ? " (runtime)" : "";
      return `${line.stat} ${formatProfileCombatBonusDelta(line.stat, line.value)} [${gate}]${runtime}`;
    })
    .join("; ");
}

function researchCombatKindLabel(kind: string): string {
  switch (kind) {
    case "flat":
      return "flat";
    case "owner_faction":
      return "owner hull";
    case "conditional":
      return "conditional";
    case "mixed":
      return "mixed";
    case "support_buff_gated":
      return "support buff";
    case "non_combat":
      return "no combat";
    case "unmapped":
      return "unmapped";
    default:
      return kind;
  }
}

export default function RosterProfileResearchSummary({
  researchSummary,
  researchSummaryError,
  researchScenarioShipId,
  onResearchScenarioShipIdChange,
  researchScenarioHostileId,
  onResearchScenarioHostileIdChange,
}: {
  researchSummary: ResearchCombatSummary | null;
  researchSummaryError: string | null;
  researchScenarioShipId: string;
  onResearchScenarioShipIdChange: (value: string) => void;
  researchScenarioHostileId: string;
  onResearchScenarioHostileIdChange: (value: string) => void;
}) {
  return (
    <>
      <h3
        style={{
          margin: "1.5rem 0 0.5rem",
          fontSize: "0.95rem",
          fontWeight: 600,
        }}
      >
        Research (sync → combat)
      </h3>
      <p
        style={{
          margin: "0 0 0.5rem",
          fontSize: "0.85rem",
          color: "var(--text-muted)",
        }}
      >
        Sync stores every <code>rid</code> + level in{" "}
        <code>research.imported.json</code> (full savepoint). The maintainer
        catalog (<code>research_catalog.json</code>) only decides which rows add
        ship combat stats in simulate/optimize; unmapped rows stay on disk for
        later mapping.
      </p>
      {researchSummaryError && (
        <p
          style={{
            margin: "0 0 0.5rem",
            fontSize: "0.85rem",
            color: "var(--error, #c44)",
          }}
        >
          {researchSummaryError}
        </p>
      )}
      {researchSummary && (
        <div style={{ marginBottom: "1rem", fontSize: "0.85rem" }}>
          {researchSummary.error && (
            <p style={{ margin: "0 0 0.5rem", color: "var(--error, #c44)" }}>
              {researchSummary.error}
            </p>
          )}
          <dl
            style={{
              margin: "0 0 0.75rem",
              display: "grid",
              gap: "0.35rem 1rem",
              gridTemplateColumns: "auto 1fr",
              maxWidth: 520,
            }}
          >
            <dt style={styles.muted}>Synced rows</dt>
            <dd style={styles.noMargin}>
              {researchSummary.synced_research_count}
            </dd>
          </dl>
          <div
            style={{
              marginBottom: "0.75rem",
              display: "grid",
              gap: "0.5rem",
              maxWidth: 520,
            }}
          >
            <div style={{ fontWeight: 600, fontSize: "0.85rem" }}>
              Scenario lens (optional)
            </div>
            <label style={{ display: "grid", gap: 4, fontSize: "0.8rem" }}>
              <span style={styles.muted}>ship_id</span>
              <input
                type="text"
                value={researchScenarioShipId}
                onChange={(e) => onResearchScenarioShipIdChange(e.target.value)}
                placeholder="e.g. uss_voyager"
                style={{ padding: "6px 8px", fontSize: "0.85rem" }}
              />
            </label>
            <label style={{ display: "grid", gap: 4, fontSize: "0.8rem" }}>
              <span style={styles.muted}>hostile_id</span>
              <input
                type="text"
                value={researchScenarioHostileId}
                onChange={(e) =>
                  onResearchScenarioHostileIdChange(e.target.value)
                }
                placeholder="e.g. hostile id or name"
                style={{ padding: "6px 8px", fontSize: "0.85rem" }}
              />
            </label>
            {researchSummary.scenario_context && (
              <p
                style={{
                  margin: 0,
                  color: "var(--text-muted)",
                  fontSize: "0.8rem",
                }}
              >
                Effective for {researchSummary.scenario_context.ship_id} vs{" "}
                {researchSummary.scenario_context.hostile_id}
                {researchSummary.scenario_context.ship_faction
                  ? ` (${researchSummary.scenario_context.ship_faction} hull vs ${researchSummary.scenario_context.defender_faction} ${researchSummary.scenario_context.defender_ship_class})`
                  : ""}
              </p>
            )}
          </div>
          {researchSummary.unmapped_research &&
            researchSummary.unmapped_research.length > 0 && (
              <div style={styles.blockGap}>
                <div style={styles.fieldLabel}>
                  Unmapped research (by level)
                </div>
                <ul
                  style={{
                    margin: 0,
                    paddingLeft: "1.25rem",
                    fontSize: "0.8rem",
                  }}
                >
                  {researchSummary.unmapped_research.map((row) => (
                    <li key={row.rid}>
                      <code>{row.rid}</code> @ level {row.level}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          {researchSummary.combat_bonuses_from_research &&
            Object.keys(researchSummary.combat_bonuses_from_research).length >
              0 && (
              <div style={styles.blockGap}>
                <div style={styles.fieldLabel}>
                  Flat combat bonuses (global)
                </div>
                <ul style={styles.bulletList}>
                  {Object.entries(researchSummary.combat_bonuses_from_research)
                    .sort(([a], [b]) => a.localeCompare(b))
                    .map(([k, v]) => (
                      <li key={k}>
                        <code>{k}</code>:{" "}
                        {formatProfileCombatBonusListValue(k, v)}
                      </li>
                    ))}
                </ul>
              </div>
            )}
          {researchSummary.combat_owner_faction_bonuses_from_research &&
            Object.keys(
              researchSummary.combat_owner_faction_bonuses_from_research,
            ).length > 0 && (
              <div style={styles.blockGap}>
                <div style={styles.fieldLabel}>Owner-hull faction bonuses</div>
                <ul style={styles.bulletList}>
                  {Object.entries(
                    researchSummary.combat_owner_faction_bonuses_from_research,
                  )
                    .sort(([a], [b]) => a.localeCompare(b))
                    .map(([faction, inner]) => (
                      <li key={faction}>
                        <code>{faction}</code>:{" "}
                        {Object.entries(inner)
                          .sort(([a], [b]) => a.localeCompare(b))
                          .map(([k, v]) => formatProfileCombatBonusEntry(k, v))
                          .join(", ")}
                      </li>
                    ))}
                </ul>
              </div>
            )}
          {researchSummary.combat_conditional_bonuses_from_research &&
            researchSummary.combat_conditional_bonuses_from_research.length >
              0 && (
              <div style={styles.blockGap}>
                <div style={styles.fieldLabel}>
                  Conditional bonuses (attack-phase seats)
                </div>
                <ul
                  style={{
                    margin: 0,
                    paddingLeft: "1.25rem",
                    fontSize: "0.8rem",
                  }}
                >
                  {researchSummary.combat_conditional_bonuses_from_research.map(
                    (line, idx) => (
                      <li key={`${line.stat}-${idx}`}>
                        <code>{line.stat}</code>{" "}
                        {formatProfileCombatBonusDelta(line.stat, line.value)} —{" "}
                        {line.condition_label ?? "conditional"}
                        {line.requires_runtime_state
                          ? " (needs morale/burning/HB in fight)"
                          : ""}
                      </li>
                    ),
                  )}
                </ul>
              </div>
            )}
          {researchSummary.combat_bonuses_scenario_effective &&
            Object.keys(researchSummary.combat_bonuses_scenario_effective)
              .length > 0 && (
              <div style={styles.blockGap}>
                <div style={styles.fieldLabel}>
                  Scenario-effective flat totals
                </div>
                <ul style={styles.bulletList}>
                  {Object.entries(
                    researchSummary.combat_bonuses_scenario_effective,
                  )
                    .sort(([a], [b]) => a.localeCompare(b))
                    .map(([k, v]) => (
                      <li key={k}>
                        <code>{k}</code>:{" "}
                        {formatProfileCombatBonusListValue(k, v)}
                      </li>
                    ))}
                </ul>
              </div>
            )}
          {researchSummary.combat_conditional_scenario_active &&
            researchSummary.combat_conditional_scenario_active.length > 0 && (
              <div style={styles.blockGap}>
                <div style={styles.fieldLabel}>
                  Conditional active for scenario (static gates)
                </div>
                <ul
                  style={{
                    margin: 0,
                    paddingLeft: "1.25rem",
                    fontSize: "0.8rem",
                  }}
                >
                  {researchSummary.combat_conditional_scenario_active.map(
                    (line, idx) => (
                      <li key={`sc-${line.stat}-${idx}`}>
                        <code>{line.stat}</code>{" "}
                        {formatProfileCombatBonusDelta(line.stat, line.value)} —{" "}
                        {line.condition_label ?? "conditional"}
                        {line.requires_runtime_state ? " (runtime gate)" : ""}
                      </li>
                    ),
                  )}
                </ul>
              </div>
            )}
          {researchSummary.research.length > 0 && (
            <div
              style={{
                overflowX: "auto",
                maxHeight: 280,
                overflowY: "auto",
                border: "1px solid var(--border)",
                borderRadius: 6,
              }}
            >
              <table
                style={{
                  width: "100%",
                  borderCollapse: "collapse",
                  fontSize: "0.8rem",
                }}
              >
                <thead>
                  <tr
                    style={{
                      textAlign: "left",
                      borderBottom: "1px solid var(--border)",
                    }}
                  >
                    <th style={styles.cellPad}>rid</th>
                    <th style={styles.cellPad}>Level</th>
                    <th style={styles.cellPad}>Research</th>
                    <th style={styles.cellPad}>Kind</th>
                    <th style={styles.cellPad}>Flat</th>
                    <th style={styles.cellPad}>Owner hull</th>
                    <th style={styles.cellPad}>Conditional</th>
                  </tr>
                </thead>
                <tbody>
                  {researchSummary.research.map((row, idx) => (
                    <tr
                      key={`${row.rid}-${idx}`}
                      style={{ borderBottom: "1px solid var(--border)" }}
                    >
                      <td
                        style={{
                          padding: "6px 8px",
                          fontFamily: "monospace",
                        }}
                      >
                        {row.rid}
                      </td>
                      <td style={styles.cellPad}>{row.level}</td>
                      <td style={styles.cellPad}>{row.research_name ?? "—"}</td>
                      <td style={styles.cellPad}>
                        {researchCombatKindLabel(row.combat_kind)}
                      </td>
                      <td
                        style={{
                          padding: "6px 8px",
                          fontFamily: "monospace",
                          fontSize: "0.75rem",
                        }}
                      >
                        {formatResearchBonusMap(row.combat_bonuses_from_row)}
                      </td>
                      <td
                        style={{
                          padding: "6px 8px",
                          fontFamily: "monospace",
                          fontSize: "0.75rem",
                        }}
                      >
                        {formatOwnerFactionResearch(
                          row.combat_owner_faction_bonuses_from_row,
                        )}
                      </td>
                      <td
                        style={{
                          padding: "6px 8px",
                          fontFamily: "monospace",
                          fontSize: "0.75rem",
                        }}
                      >
                        {formatConditionalResearch(
                          row.combat_conditional_bonuses_from_row,
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}
    </>
  );
}
