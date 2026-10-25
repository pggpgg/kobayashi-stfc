import type {
  OptimizerSettings,
  UpdateOptimizerSettings,
} from "../../lib/useOptimizerSettings";
import {
  checkboxLabelStyle,
  FieldLabelWithHint,
  HelpHint,
  selectStyle,
  settingUpdater,
} from "./ui";

interface Props {
  settings: Pick<
    OptimizerSettings,
    | "optimizerStrategy"
    | "enableLearnedPairPrior"
    | "localRefinement"
    | "tieredScoutSims"
    | "tieredTopK"
    | "tieredRandomExplorationPct"
  >;
  onChange: UpdateOptimizerSettings;
}
export default function StrategySection({ settings, onChange }: Props) {
  const {
    optimizerStrategy,
    enableLearnedPairPrior,
    localRefinement,
    tieredScoutSims,
    tieredTopK,
    tieredRandomExplorationPct,
  } = settings;
  const update = settingUpdater(onChange);
  const MAX_TIERED_SCOUT_SIMS_UI = 100_000;
  const MAX_TIERED_TOP_K_UI = 500;
  return (
    <>
      {/* ── Optimizer strategy ───────────────────────────────────── */}
      <div style={{ fontSize: "0.85rem" }}>
        <FieldLabelWithHint
          hint={
            "Exhaustive: generate candidate crews from officer pools (respecting Max crews when set), then run your full Sims per crew on each. Genetic: evolve populations of crews for very large spaces; ignores the Max crews cap. Tiered: a fast scouting pass with fewer sims per crew ranks candidates, then your full sim count runs only on the best Top K crews. Random stratified: baseline control — random legal crews stratified across captain faction/rarity and below-decks families, then scout → confirm."
          }
        >
          <span>Search method</span>
        </FieldLabelWithHint>
        <select
          value={optimizerStrategy}
          onChange={(e) =>
            update("optimizerStrategy")(
              e.target.value as import("../../lib/api").OptimizerStrategyType,
            )
          }
          style={selectStyle}
        >
          <option value="exhaustive">Try every candidate</option>
          <option value="genetic">Evolve promising crews</option>
          <option value="tiered">Scout, then verify finalists</option>
          <option value="linear_eval">Estimate expected damage</option>
          <option value="random_stratified">Sample varied crews</option>
        </select>
        {optimizerStrategy === "random_stratified" && (
          <p
            style={{
              margin: "4px 0 0",
              fontSize: "0.72rem",
              color: "var(--text-muted)",
            }}
          >
            Samples a varied set of legal crews, then verifies the finalists.
          </p>
        )}
        {optimizerStrategy === "linear_eval" && (
          <p
            style={{
              margin: "4px 0 0",
              fontSize: "0.72rem",
              color: "var(--text-muted)",
            }}
          >
            Ranks crews by estimated hull damage instead of simulating each
            fight.
          </p>
        )}
      </div>
      {optimizerStrategy !== "linear_eval" && (
        <label style={checkboxLabelStyle}>
          <input
            type="checkbox"
            checked={enableLearnedPairPrior}
            onChange={(e) => update("enableLearnedPairPrior")(e.target.checked)}
            style={{ margin: 0 }}
          />
          <span
            style={{ display: "inline-flex", alignItems: "center", gap: 4 }}
          >
            Favor officer pairs that worked before
            <HelpHint text="Analytical prefilter only: gives a small boost to crews whose officer pairs frequently co-occur in warm-start/history reference crews. Turn off to remove this learned tie-breaker while keeping other priors." />
          </span>
        </label>
      )}
      {(optimizerStrategy === "tiered" || optimizerStrategy === "genetic") && (
        <label style={checkboxLabelStyle}>
          <input
            type="checkbox"
            checked={localRefinement}
            onChange={(e) => update("localRefinement")(e.target.checked)}
            style={{ margin: 0 }}
          />
          <span
            style={{ display: "inline-flex", alignItems: "center", gap: 4 }}
          >
            Refine finalists
            <HelpHint text="After the main search finishes, hill-climb around its top crews: try every legal one-officer substitution, captain swaps that keep the support officers, and rebuilds of two seats at once. Only improvements that survive full confirmation sims are kept, and each one reports which officer changed and how much it gained. Costs a small extra slice of sims on top of the main search." />
          </span>
        </label>
      )}

      {optimizerStrategy === "tiered" && (
        <>
          <div style={{ fontSize: "0.85rem" }}>
            <FieldLabelWithHint
              hint={
                "Number of Monte Carlo trials per crew during the tiered scouting phase (always fewer than your main Sims per crew). Lower is faster but ranks candidates more noisily. Leave blank for the server default (typically 500)."
              }
            >
              <span>Quick trials per crew (optional)</span>
            </FieldLabelWithHint>
            <input
              type="number"
              min={1}
              max={MAX_TIERED_SCOUT_SIMS_UI}
              step={1}
              placeholder="500 (default)"
              value={tieredScoutSims ?? ""}
              onChange={(e) => {
                const raw = e.target.value.trim();
                if (raw === "") {
                  update("tieredScoutSims")(null);
                  return;
                }
                const n = parseInt(raw, 10);
                if (!Number.isNaN(n) && n >= 1) {
                  update("tieredScoutSims")(
                    Math.min(n, MAX_TIERED_SCOUT_SIMS_UI),
                  );
                }
              }}
              style={selectStyle}
            />
          </div>
          <div style={{ fontSize: "0.85rem" }}>
            <FieldLabelWithHint
              hint={
                "How many top crews from the scout pass are promoted to the confirmation phase, where each receives your full Sims per crew. Leave blank for the server default (typically 20)."
              }
            >
              <span>Finalists to verify (optional)</span>
            </FieldLabelWithHint>
            <input
              type="number"
              min={1}
              max={MAX_TIERED_TOP_K_UI}
              step={1}
              placeholder="20 (default)"
              value={tieredTopK ?? ""}
              onChange={(e) => {
                const raw = e.target.value.trim();
                if (raw === "") {
                  update("tieredTopK")(null);
                  return;
                }
                const n = parseInt(raw, 10);
                if (!Number.isNaN(n) && n >= 1) {
                  update("tieredTopK")(Math.min(n, MAX_TIERED_TOP_K_UI));
                }
              }}
              style={selectStyle}
            />
          </div>
          <div style={{ fontSize: "0.85rem" }}>
            <FieldLabelWithHint
              hint={
                "Fraction of scout candidates (0–50%) swapped for random legal crews stratified by captain faction/rarity and below-decks family. These bypass the analytical prefilter, so surprising crews the proxy would discard still get scouted. Rows they produce are labeled random_stratified. Leave blank to disable."
              }
            >
              <span>Random exploration % (optional)</span>
            </FieldLabelWithHint>
            <input
              type="number"
              min={0}
              max={50}
              step={1}
              placeholder="off (default)"
              value={
                tieredRandomExplorationPct != null
                  ? Math.round(tieredRandomExplorationPct * 100)
                  : ""
              }
              onChange={(e) => {
                const raw = e.target.value.trim();
                if (raw === "") {
                  update("tieredRandomExplorationPct")(null);
                  return;
                }
                const n = parseInt(raw, 10);
                if (!Number.isNaN(n) && n >= 0) {
                  update("tieredRandomExplorationPct")(Math.min(n, 50) / 100);
                }
              }}
              style={selectStyle}
            />
          </div>
          <p
            style={{
              margin: "4px 0 0",
              fontSize: "0.72rem",
              color: "var(--text-muted)",
            }}
          >
            Leave blank for server defaults (500 scout sims, top 20, no random
            exploration). Max {MAX_TIERED_SCOUT_SIMS_UI.toLocaleString()} /{" "}
            {MAX_TIERED_TOP_K_UI}.
          </p>
        </>
      )}
    </>
  );
}
