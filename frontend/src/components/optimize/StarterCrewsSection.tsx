import { useCallback } from "react";
import type {
  OptimizerSettings,
  UpdateOptimizerSettings,
} from "../../lib/useOptimizerSettings";
import {
  checkboxLabelStyle,
  checkboxResetStyle,
  HelpHint,
  selectStyle,
  settingUpdater,
} from "./ui";

interface Props {
  settings: Pick<
    OptimizerSettings,
    | "selectedSeeds"
    | "heuristicsOnly"
    | "fastDiscovery"
    | "belowDecksStrategy"
    | "optimizerStrategy"
  >;
  onChange: UpdateOptimizerSettings;
  availableSeeds: string[];
}
export default function StarterCrewsSection({
  settings,
  onChange,
  availableSeeds,
}: Props) {
  const {
    selectedSeeds,
    heuristicsOnly,
    fastDiscovery,
    belowDecksStrategy,
    optimizerStrategy,
  } = settings;
  const update = settingUpdater(onChange);
  const toggleSeed = useCallback(
    (seed: string) => {
      onChange({
        selectedSeeds: selectedSeeds.includes(seed)
          ? selectedSeeds.filter((s) => s !== seed)
          : [...selectedSeeds, seed],
      });
    },
    [selectedSeeds, onChange],
  );
  return (
    <>
      {/* ── Starter crews ─────────────────────────────────────── */}
      <div>
        <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
          <div style={{ fontSize: "0.85rem", fontWeight: 600 }}>
            Starter crews
          </div>
          <HelpHint
            text={
              "Named starter crew lists the optimizer evaluates before the broader search, so you can steer results toward lineups you already trust. Add seed files under data/heuristics in this repository (the server exposes discovered seed names here)."
            }
          />
        </div>
        {availableSeeds.length > 0 ? (
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: "0.3rem",
              maxHeight: 120,
              overflowY: "auto",
              border: "1px solid var(--border)",
              borderRadius: 4,
              padding: "0.4rem",
              marginTop: 4,
            }}
          >
            {availableSeeds.map((seed) => (
              <label key={seed} style={checkboxLabelStyle}>
                <input
                  type="checkbox"
                  checked={selectedSeeds.includes(seed)}
                  onChange={() => toggleSeed(seed)}
                  style={checkboxResetStyle}
                />
                <span style={{ fontSize: "0.8rem" }}>{seed}</span>
              </label>
            ))}
          </div>
        ) : (
          <p
            style={{
              margin: "4px 0 0",
              fontSize: "0.75rem",
              color: "var(--text-muted)",
            }}
          >
            No starter crew lists are available.
          </p>
        )}
      </div>

      {/* ── Below-decks selection (shown when seeds selected) ─────── */}
      {selectedSeeds.length > 0 && (
        <>
          <label style={{ fontSize: "0.85rem" }}>
            Below-decks selection
            <select
              value={belowDecksStrategy}
              onChange={(e) =>
                update("belowDecksStrategy")(
                  e.target.value as "ordered" | "exploration",
                )
              }
              style={selectStyle}
            >
              <option value="ordered">Use listed officers in order</option>
              <option value="exploration">
                Try combinations from the list
              </option>
            </select>
          </label>

          <label style={checkboxLabelStyle}>
            <input
              type="checkbox"
              checked={heuristicsOnly}
              onChange={(e) => update("heuristicsOnly")(e.target.checked)}
              style={checkboxResetStyle}
            />
            <span>Use only starter crews</span>
          </label>

          <label
            style={{
              ...checkboxLabelStyle,
              opacity: optimizerStrategy === "genetic" ? 0.45 : 1,
            }}
          >
            <input
              type="checkbox"
              checked={fastDiscovery}
              disabled={optimizerStrategy === "genetic"}
              onChange={(e) => update("fastDiscovery")(e.target.checked)}
              aria-label="Include starter crews in the main search"
              style={checkboxResetStyle}
            />
            <span
              style={{ display: "inline-flex", alignItems: "center", gap: 4 }}
            >
              Include starter crews in the main search
              <HelpHint
                text={
                  optimizerStrategy === "genetic"
                    ? "Not available with genetic strategy (use seeds without this option for seeded GA)."
                    : "Seed crews are prepended to the optimizer warm-start list so they go through the same approximate analytical rank and Monte Carlo path as generated crews, instead of a separate full-sim pass on every seed combination first."
                }
              />
            </span>
          </label>
        </>
      )}
    </>
  );
}
