import type {
  OptimizerSettings,
  UpdateOptimizerSettings,
} from "../../lib/useOptimizerSettings";
import {
  CollapsibleSection,
  checkboxLabelStyle,
  FieldLabelWithHint,
  selectStyle,
  settingUpdater,
} from "./ui";

interface Props {
  settings: Pick<
    OptimizerSettings,
    | "noveltyLambdaText"
    | "noveltyDiverseTopText"
    | "noveltyPoolText"
    | "noveltyHistoryAnchors"
  >;
  onChange: UpdateOptimizerSettings;
}
export default function DiversitySection({ settings, onChange }: Props) {
  const {
    noveltyLambdaText,
    noveltyDiverseTopText,
    noveltyPoolText,
    noveltyHistoryAnchors,
  } = settings;
  const update = settingUpdater(onChange);
  const MAX_NOVELTY_DIVERSE_TOP_UI = 500;
  const MAX_NOVELTY_POOL_UI = 10_000;
  return (
    <CollapsibleSection title="Recommendation variety">
      <div style={{ fontSize: "0.85rem" }}>
        <FieldLabelWithHint
          hint={
            "Maximal marginal relevance (MMR) on officer sets: λ in (0, 1] blends win rate vs redundancy. Higher λ stays closer to pure strength order. Leave blank to disable. Optional head size and pool limit refine how many leading rows are reordered and how wide the candidate pool is (server defaults apply when omitted)."
          }
        >
          <span>Strength preference (optional, 0–1)</span>
        </FieldLabelWithHint>
        <input
          type="text"
          inputMode="decimal"
          placeholder="Off — pure strength order"
          value={noveltyLambdaText}
          onChange={(e) => update("noveltyLambdaText")(e.target.value)}
          aria-label="Strength preference for varied recommendations"
          style={selectStyle}
        />
        <label
          style={{
            fontSize: "0.85rem",
            display: "block",
            marginTop: 8,
            opacity: noveltyLambdaText.trim() ? 1 : 0.45,
          }}
        >
          Varied recommendations to show (optional)
          <input
            type="number"
            min={1}
            max={MAX_NOVELTY_DIVERSE_TOP_UI}
            step={1}
            placeholder="20 when enabled"
            disabled={!noveltyLambdaText.trim()}
            value={noveltyDiverseTopText}
            onChange={(e) => update("noveltyDiverseTopText")(e.target.value)}
            style={selectStyle}
          />
        </label>
        <label
          style={{
            fontSize: "0.85rem",
            display: "block",
            marginTop: 8,
            opacity: noveltyLambdaText.trim() ? 1 : 0.45,
          }}
        >
          Recommendations to consider (optional)
          <input
            type="number"
            min={2}
            max={MAX_NOVELTY_POOL_UI}
            step={1}
            placeholder="Automatic when enabled"
            disabled={!noveltyLambdaText.trim()}
            value={noveltyPoolText}
            onChange={(e) => update("noveltyPoolText")(e.target.value)}
            style={selectStyle}
          />
        </label>
        <label
          style={{
            ...checkboxLabelStyle,
            marginTop: 10,
            opacity: noveltyLambdaText.trim() ? 1 : 0.45,
          }}
        >
          <input
            type="checkbox"
            checked={noveltyHistoryAnchors}
            disabled={!noveltyLambdaText.trim()}
            onChange={(e) => update("noveltyHistoryAnchors")(e.target.checked)}
            aria-label="Prefer different officers from previous recommendations"
          />
          <span>Prefer different officers from previous recommendations</span>
        </label>
        <p
          style={{
            margin: "6px 0 0",
            fontSize: "0.72rem",
            color: "var(--text-muted)",
          }}
        >
          Set a strength preference to enable variety. The number considered
          must be at least the number shown. Maximum shown:{" "}
          {MAX_NOVELTY_DIVERSE_TOP_UI}; considered:{" "}
          {MAX_NOVELTY_POOL_UI.toLocaleString()}.
        </p>
      </div>
    </CollapsibleSection>
  );
}
