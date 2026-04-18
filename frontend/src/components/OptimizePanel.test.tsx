import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CrewState } from "../lib/types";
import OptimizePanel from "./OptimizePanel";

const emptyCrew: CrewState = {
  captain: null,
  bridge: [null, null],
  belowDeck: [null, null, null],
};

const baseProps = {
  collapsed: false,
  onToggleCollapsed: vi.fn(),
  officerOptions: [] as import("../lib/api").OfficerListItem[],
  crew: emptyCrew,
  loadingOptimize: false,
  optimizeCrewsDone: null as number | null,
  optimizeTotalCrews: null as number | null,
  maxCandidates: null as number | null,
  onMaxCandidatesChange: vi.fn(),
  prioritizeBelowDecksAbility: false,
  onPrioritizeBelowDecksAbilityChange: vi.fn(),
  availableSeeds: [] as string[],
  selectedSeeds: [] as string[],
  onSelectedSeedsChange: vi.fn(),
  heuristicsOnly: false,
  onHeuristicsOnlyChange: vi.fn(),
  belowDecksStrategy: "ordered" as const,
  onBelowDecksStrategyChange: vi.fn(),
  optimizerStrategy: "tiered" as const,
  onOptimizerStrategyChange: vi.fn(),
  tieredScoutSims: null as number | null,
  onTieredScoutSimsChange: vi.fn(),
  tieredTopK: null as number | null,
  onTieredTopKChange: vi.fn(),
  optimizeMustInclude: "",
  onOptimizeMustIncludeChange: vi.fn(),
  optimizeExclude: "",
  onOptimizeExcludeChange: vi.fn(),
  optimizeCaptainMust: "",
  onOptimizeCaptainMustChange: vi.fn(),
  optimizeBridgeMust: "",
  onOptimizeBridgeMustChange: vi.fn(),
  optimizeBelowMust: "",
  onOptimizeBelowMustChange: vi.fn(),
  optimizeGroupsJson: "",
  onOptimizeGroupsJsonChange: vi.fn(),
  chainGrindEnabled: false,
  onChainGrindEnabledChange: vi.fn(),
  chainKillsTarget: 3,
  onChainKillsTargetChange: vi.fn(),
  chainSecondary: "min_hull_damage" as const,
  onChainSecondaryChange: vi.fn(),
};

describe("OptimizePanel", () => {
  it("renders expanded panel with Strategy heading", () => {
    render(<OptimizePanel {...baseProps} />);
    expect(screen.getByText("Strategy")).toBeTruthy();
  });

  it("renders collapsed state with expand button", () => {
    render(<OptimizePanel {...baseProps} collapsed={true} />);
    expect(screen.getByLabelText("Expand panel")).toBeTruthy();
  });

  it("calls onToggleCollapsed when collapse button clicked", () => {
    const fn = vi.fn();
    render(<OptimizePanel {...baseProps} onToggleCollapsed={fn} />);
    fireEvent.click(screen.getByLabelText("Collapse panel"));
    expect(fn).toHaveBeenCalledOnce();
  });

  it("shows heuristic seeds when available", () => {
    render(
      <OptimizePanel
        {...baseProps}
        availableSeeds={["swarm-crews", "borg-crews"]}
      />,
    );
    expect(screen.getByText("Heuristics seeds")).toBeTruthy();
    expect(screen.getByText("swarm-crews")).toBeTruthy();
    expect(screen.getByText("borg-crews")).toBeTruthy();
  });

  it("shows heuristics seeds heading and hint when no seeds", () => {
    render(<OptimizePanel {...baseProps} availableSeeds={[]} />);
    expect(screen.getByText("Heuristics seeds")).toBeTruthy();
    expect(
      screen.getByRole("note", {
        name: /data\/heuristics/i,
      }),
    ).toBeTruthy();
    expect(screen.getByText(/No seeds were returned/i)).toBeTruthy();
  });

  it("shows below-decks strategy when seeds are selected", () => {
    render(
      <OptimizePanel
        {...baseProps}
        availableSeeds={["swarm-crews"]}
        selectedSeeds={["swarm-crews"]}
      />,
    );
    expect(screen.getByText("Below-decks strategy")).toBeTruthy();
    expect(screen.getByText(/Heuristics only/)).toBeTruthy();
  });

  it("calls onMaxCandidatesChange when input changes", () => {
    const fn = vi.fn();
    render(<OptimizePanel {...baseProps} onMaxCandidatesChange={fn} />);
    const input = screen.getByPlaceholderText("No limit");
    fireEvent.change(input, { target: { value: "500" } });
    expect(fn).toHaveBeenCalledWith(500);
  });

  it("clamps max candidates to 2,000,000", () => {
    const fn = vi.fn();
    render(<OptimizePanel {...baseProps} onMaxCandidatesChange={fn} />);
    const input = screen.getByPlaceholderText("No limit");
    fireEvent.change(input, { target: { value: "9999999" } });
    expect(fn).toHaveBeenCalledWith(2_000_000);
  });

  it("sets maxCandidates to null when input cleared", () => {
    const fn = vi.fn();
    render(
      <OptimizePanel
        {...baseProps}
        maxCandidates={100}
        onMaxCandidatesChange={fn}
      />,
    );
    const input = screen.getByPlaceholderText("No limit");
    fireEvent.change(input, { target: { value: "" } });
    expect(fn).toHaveBeenCalledWith(null);
  });

  it("toggles prioritize below-decks checkbox", () => {
    const fn = vi.fn();
    render(
      <OptimizePanel {...baseProps} onPrioritizeBelowDecksAbilityChange={fn} />,
    );
    const checkbox = screen.getByRole("checkbox", {
      name: /Only below-decks officers with ability/,
    });
    fireEvent.click(checkbox);
    expect(fn).toHaveBeenCalledWith(true);
  });

  it("shows live status during optimization", () => {
    render(
      <OptimizePanel
        {...baseProps}
        loadingOptimize={true}
        optimizeCrewsDone={50}
        optimizeTotalCrews={200}
      />,
    );
    expect(screen.getByText(/Live: units 50 \/ 200/)).toBeTruthy();
  });

  it("shows tiered scout/top-K fields when strategy is tiered", () => {
    render(<OptimizePanel {...baseProps} optimizerStrategy="tiered" />);
    expect(screen.getByPlaceholderText("500 (default)")).toBeTruthy();
    expect(screen.getByPlaceholderText("20 (default)")).toBeTruthy();
  });

  it("hides tiered scout/top-K fields when strategy is not tiered", () => {
    render(<OptimizePanel {...baseProps} optimizerStrategy="exhaustive" />);
    expect(screen.queryByPlaceholderText("500 (default)")).toBeNull();
    expect(screen.queryByPlaceholderText("20 (default)")).toBeNull();
  });

  it("calls onTieredScoutSimsChange and clamps to 100,000", () => {
    const fn = vi.fn();
    render(
      <OptimizePanel
        {...baseProps}
        onTieredScoutSimsChange={fn}
        optimizerStrategy="tiered"
      />,
    );
    const input = screen.getByPlaceholderText("500 (default)");
    fireEvent.change(input, { target: { value: "2000" } });
    expect(fn).toHaveBeenCalledWith(2000);
    fireEvent.change(input, { target: { value: "999999" } });
    expect(fn).toHaveBeenCalledWith(100_000);
  });

  it("clears tiered scout sims when input emptied", () => {
    const fn = vi.fn();
    render(
      <OptimizePanel
        {...baseProps}
        tieredScoutSims={100}
        onTieredScoutSimsChange={fn}
        optimizerStrategy="tiered"
      />,
    );
    fireEvent.change(screen.getByPlaceholderText("500 (default)"), {
      target: { value: "" },
    });
    expect(fn).toHaveBeenCalledWith(null);
  });
});
