import type { BuildingCombatSummary } from "../lib/api";
import { formatProfileCombatBonusListValue } from "../lib/profileCombatBonusDisplay";
import { styles } from "../lib/rosterProfileStyles";

export default function RosterProfileBuildingsSummary({
  buildingSummary,
  buildingSummaryError,
}: {
  buildingSummary: BuildingCombatSummary | null;
  buildingSummaryError: string | null;
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
        Buildings (sync → combat)
      </h3>
      <p
        style={{
          margin: "0 0 0.5rem",
          fontSize: "0.85rem",
          color: "var(--text-muted)",
        }}
      >
        Starbase modules from sync (<code>buildings.imported.json</code>) and
        the combat stat bonuses they contribute in ship combat (same rules as
        simulate/optimize). Set ops level override under Player Bonuses if you
        need it without sync.
      </p>
      {buildingSummaryError && (
        <p
          style={{
            margin: "0 0 0.5rem",
            fontSize: "0.85rem",
            color: "var(--error, #c44)",
          }}
        >
          {buildingSummaryError}
        </p>
      )}
      {buildingSummary && (
        <div style={{ marginBottom: "1rem", fontSize: "0.85rem" }}>
          {buildingSummary.error && (
            <p style={{ margin: "0 0 0.5rem", color: "var(--error, #c44)" }}>
              {buildingSummary.error}
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
              {buildingSummary.synced_building_count}
            </dd>
            <dt style={styles.muted}>Ops (profile override)</dt>
            <dd style={styles.noMargin}>
              {buildingSummary.ops_level_profile_override ?? "—"}
            </dd>
            <dt style={styles.muted}>Ops (inferred from sync)</dt>
            <dd style={styles.noMargin}>
              {buildingSummary.ops_level_inferred_from_sync ?? "—"}
            </dd>
            <dt style={styles.muted}>Ops (effective)</dt>
            <dd style={styles.noMargin}>
              {buildingSummary.ops_level_effective ?? "—"}
            </dd>
          </dl>
          {buildingSummary.unmapped_bids.length > 0 && (
            <p style={{ margin: "0 0 0.5rem", color: "var(--text-muted)" }}>
              Unmapped game <code>bid</code> values (no catalog entry):{" "}
              {buildingSummary.unmapped_bids.join(", ")}
            </p>
          )}
          {buildingSummary.combat_bonuses_from_buildings &&
            Object.keys(buildingSummary.combat_bonuses_from_buildings).length >
              0 && (
              <div style={styles.blockGap}>
                <div style={styles.fieldLabel}>
                  Combat bonuses from buildings
                </div>
                <ul style={styles.bulletList}>
                  {Object.entries(buildingSummary.combat_bonuses_from_buildings)
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
          {buildingSummary.buildings.length > 0 && (
            <div
              style={{
                overflowX: "auto",
                maxHeight: 240,
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
                    <th style={styles.cellPad}>bid</th>
                    <th style={styles.cellPad}>Level</th>
                    <th style={styles.cellPad}>Building</th>
                    <th style={styles.cellPad}>Catalog</th>
                  </tr>
                </thead>
                <tbody>
                  {buildingSummary.buildings.map((row) => (
                    <tr
                      key={row.bid}
                      style={{ borderBottom: "1px solid var(--border)" }}
                    >
                      <td
                        style={{
                          padding: "6px 8px",
                          fontFamily: "monospace",
                        }}
                      >
                        {row.bid}
                      </td>
                      <td style={styles.cellPad}>{row.level}</td>
                      <td style={styles.cellPad}>
                        {row.building_name ?? row.kobayashi_building_id ?? "—"}
                      </td>
                      <td style={styles.cellPad}>
                        {row.catalog_record_present ? "yes" : "no"}
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
