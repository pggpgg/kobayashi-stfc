import type {
  OptimizerSettings,
  UpdateOptimizerSettings,
} from "../../lib/useOptimizerSettings";
import {
  CollapsibleSection,
  checkboxLabelStyle,
  checkboxResetStyle,
  inputStyleCompact,
  selectStyle,
  settingUpdater,
} from "./ui";

interface Props {
  settings: Pick<
    OptimizerSettings,
    | "optimizerStrategy"
    | "chainGrindEnabled"
    | "chainKillsTarget"
    | "chainSecondary"
  >;
  onChange: UpdateOptimizerSettings;
}
export default function ChainSection({ settings, onChange }: Props) {
  const {
    optimizerStrategy,
    chainGrindEnabled,
    chainKillsTarget,
    chainSecondary,
  } = settings;
  const update = settingUpdater(onChange);

  return (
    <>
      {optimizerStrategy !== "linear_eval" && (
        <CollapsibleSection title="Chain grind">
          <div>
            <label style={checkboxLabelStyle}>
              <input
                type="checkbox"
                checked={chainGrindEnabled}
                onChange={(e) => update("chainGrindEnabled")(e.target.checked)}
                style={checkboxResetStyle}
              />
              <span>
                Chain grind (N wins, hull carries, shields full each fight)
              </span>
            </label>
            {chainGrindEnabled && (
              <>
                <label
                  style={{
                    fontSize: "0.85rem",
                    display: "block",
                    marginTop: 8,
                  }}
                >
                  Kills target (N)
                  <input
                    type="number"
                    min={1}
                    max={50}
                    value={chainKillsTarget}
                    onChange={(e) =>
                      update("chainKillsTarget")(
                        Math.min(50, Math.max(1, Number(e.target.value) || 1)),
                      )
                    }
                    style={inputStyleCompact}
                  />
                </label>
                <label
                  style={{
                    fontSize: "0.85rem",
                    display: "block",
                    marginTop: 8,
                  }}
                >
                  Secondary (tie-break after chain success rate)
                  <select
                    value={chainSecondary}
                    onChange={(e) =>
                      update("chainSecondary")(
                        e.target.value as
                          | "min_hull_damage"
                          | "max_loot_per_hull_proxy",
                      )
                    }
                    style={selectStyle}
                  >
                    <option value="min_hull_damage">
                      Max hull after N kills (min damage)
                    </option>
                    <option value="max_loot_per_hull_proxy">
                      Loot / hull proxy (placeholder)
                    </option>
                  </select>
                </label>
                <p
                  style={{
                    margin: "6px 0 0",
                    fontSize: "0.72rem",
                    color: "var(--text-muted)",
                  }}
                >
                  Repeats the same hostile. Results show how often the crew
                  completes the chain and how much hull remains.
                </p>
              </>
            )}
          </div>
        </CollapsibleSection>
      )}
    </>
  );
}
