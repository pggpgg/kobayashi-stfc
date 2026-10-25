import type {
  OptimizerSettings,
  UpdateOptimizerSettings,
} from "../../lib/useOptimizerSettings";
import {
  CollapsibleSection,
  HelpHint,
  selectStyle,
  settingUpdater,
} from "./ui";

interface Props {
  settings: Pick<OptimizerSettings, "maxCandidates" | "belowDecksPoolMode">;
  onChange: UpdateOptimizerSettings;
}
export default function SearchScopeSection({ settings, onChange }: Props) {
  const { maxCandidates, belowDecksPoolMode } = settings;
  const update = settingUpdater(onChange);

  return (
    <CollapsibleSection title="Search scope">
      <label style={{ fontSize: "0.85rem" }}>
        Max crews (optional)
        <input
          type="number"
          min={1}
          max={2_000_000}
          step={1}
          placeholder="Auto limit (10,000)"
          value={maxCandidates ?? ""}
          onChange={(e) => {
            const raw = e.target.value.trim();
            if (raw === "") {
              update("maxCandidates")(null);
              return;
            }
            const n = parseInt(raw, 10);
            if (!Number.isNaN(n) && n >= 1) {
              update("maxCandidates")(Math.min(n, 2_000_000));
            }
          }}
          style={selectStyle}
        />
      </label>

      <p style={{ fontSize: "0.75rem", color: "var(--text-muted)", margin: 0 }}>
        Blank uses the server&apos;s 10,000-candidate safety limit. An explicit
        value changes that limit; genetic search uses its own population
        settings.
      </p>

      <label style={{ fontSize: "0.85rem" }}>
        Below-decks officer pool
        <HelpHint text="Strict (default): only officers with a canonical combat-modifier below-decks ability — narrow pool, fastest runs. Scored: include all below-decks-ability officers, ranked by combat relevance (combat → ambiguous → economy-only) with LCARS Attack+Defense+Health as tiebreaker — gives officers with unannotated abilities a chance without flooding the pool. Relaxed: any officer can fill a below-decks slot, ranked by power — widest pool, useful when the combat-only pool is thin or for exploration." />
        <select
          value={belowDecksPoolMode}
          onChange={(e) =>
            update("belowDecksPoolMode")(
              e.target
                .value as import("../../lib/optimizeWarmStart").BelowDecksPoolMode,
            )
          }
          style={selectStyle}
        >
          <option value="strict">
            Strict — combat BD first, then by power
          </option>
          <option value="scored">Scored — rank by combat relevance</option>
          <option value="relaxed">Relaxed — any officer, by power</option>
        </select>
      </label>
    </CollapsibleSection>
  );
}
