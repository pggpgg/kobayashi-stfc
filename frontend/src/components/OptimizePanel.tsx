import { memo } from "react";
import type { OfficerListItem } from "../lib/api";
import type {
  OptimizerSettings,
  UpdateOptimizerSettings,
} from "../lib/useOptimizerSettings";
import ChainSection from "./optimize/ChainSection";
import ConstraintsSection from "./optimize/ConstraintsSection";
import DiversitySection from "./optimize/DiversitySection";
import ProgressSection, {
  type OptimizerProgress,
} from "./optimize/ProgressSection";
import SearchScopeSection from "./optimize/SearchScopeSection";
import StarterCrewsSection from "./optimize/StarterCrewsSection";
import StrategySection from "./optimize/StrategySection";
import { iconButtonStyle } from "./optimize/ui";

interface OptimizePanelProps {
  collapsed: boolean;
  onToggleCollapsed: () => void;
  officerOptions: OfficerListItem[];
  availableSeeds: string[];
  settings: OptimizerSettings;
  onChange: UpdateOptimizerSettings;
  progress: OptimizerProgress;
}

export default memo(function OptimizePanel({
  collapsed,
  onToggleCollapsed,
  officerOptions,
  availableSeeds,
  settings,
  onChange,
  progress,
}: OptimizePanelProps) {
  if (collapsed) {
    return (
      <aside
        style={{
          width: 48,
          background: "var(--surface)",
          borderLeft: "1px solid var(--border)",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          padding: "0.5rem",
        }}
      >
        <button
          type="button"
          onClick={onToggleCollapsed}
          aria-label="Expand panel"
          style={iconButtonStyle}
        >
          →
        </button>
        <span
          style={{ fontSize: 10, color: "var(--text-muted)", marginTop: 8 }}
        >
          Strategy
        </span>
        <span style={{ fontSize: 10, color: "var(--text-muted)" }}>—</span>
      </aside>
    );
  }

  return (
    <aside
      className="optimize-panel"
      style={{
        background: "var(--surface)",
        padding: "1rem",
        display: "flex",
        flexDirection: "column",
        gap: "0.75rem",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <h2 style={{ margin: 0, fontSize: "1rem" }}>Strategy</h2>
        <button
          type="button"
          onClick={onToggleCollapsed}
          aria-label="Collapse panel"
          style={iconButtonStyle}
        >
          ←
        </button>
      </div>

      <StarterCrewsSection
        settings={settings}
        onChange={onChange}
        availableSeeds={availableSeeds}
      />
      <StrategySection settings={settings} onChange={onChange} />
      <p style={{ margin: 0, fontSize: "0.8rem", color: "var(--text-muted)" }}>
        {settings.optimizerStrategy === "linear_eval"
          ? "Ranking: estimated hull damage dealt, highest first."
          : settings.chainGrindEnabled
            ? "Ranking: chain success rate first, then secondary among successful trials."
            : "Ranking: 80% win rate + 20% average hull remaining."}
      </p>

      <ChainSection settings={settings} onChange={onChange} />
      <DiversitySection settings={settings} onChange={onChange} />
      <ConstraintsSection
        settings={settings}
        onChange={onChange}
        officerOptions={officerOptions}
      />
      <SearchScopeSection settings={settings} onChange={onChange} />
      <ProgressSection {...progress} />
    </aside>
  );
});
