import { fireEvent, render, screen, within } from "@testing-library/react";
import { type ComponentProps, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import {
  defaultOptimizerSettings,
  type OptimizerSettings,
} from "../lib/useOptimizerSettings";
import OptimizePanel from "./OptimizePanel";

type PanelProps = ComponentProps<typeof OptimizePanel>;
const baseProps: PanelProps = {
  collapsed: false,
  onToggleCollapsed: vi.fn(),
  officerOptions: [],
  availableSeeds: [],
  settings: { ...defaultOptimizerSettings(), maxCandidates: null },
  onChange: vi.fn(),
  progress: {
    loadingOptimize: false,
    optimizeCrewsDone: null,
    optimizeTotalCrews: null,
  },
};
function renderPanel(
  settings: Partial<OptimizerSettings> = {},
  props: Partial<PanelProps> = {},
) {
  return render(
    <OptimizePanel
      {...baseProps}
      {...props}
      settings={{ ...baseProps.settings, ...settings }}
    />,
  );
}
function GroupsHarness({ initial = "" }: { initial?: string }) {
  const [settings, setSettings] = useState(() => ({
    ...defaultOptimizerSettings(),
    optimizeGroupsJson: initial,
  }));
  return (
    <>
      <OptimizePanel
        {...baseProps}
        settings={settings}
        onChange={(patch) =>
          setSettings((previous) => ({ ...previous, ...patch }))
        }
        officerOptions={[
          { id: "a", name: "Picard", slot: "captain" },
          { id: "b", name: "Data", slot: "bridge" },
          { id: "c", name: "Riker", slot: "bridge" },
        ]}
      />
      <output data-testid="saved-groups">{settings.optimizeGroupsJson}</output>
    </>
  );
}
function renderGroups(initial = "") {
  const view = render(<GroupsHarness initial={initial} />);
  const summary = screen.getByText("Search constraints");
  if (!summary.closest("details")?.open) fireEvent.click(summary);
  return view;
}

describe("#302 officer groups and recommendation wording", () => {
  it("starts empty with an add action and no raw JSON editor", () => {
    renderGroups();
    expect(
      screen.getByRole("button", { name: "Add officer group" }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("textbox", { name: /Groups \(JSON\)/ }),
    ).toBeNull();
  });
  it("adds searchable officers and changes the required count using the existing wire structure", () => {
    render(<GroupsHarness />);
    fireEvent.click(screen.getByRole("button", { name: "Add officer group" }));
    const group = screen.getByRole("group", { name: "Officer group 1" });
    fireEvent.change(within(group).getByRole("combobox"), {
      target: { value: "pica" },
    });
    fireEvent.click(within(group).getByRole("option", { name: "Picard" }));
    fireEvent.change(within(group).getByRole("combobox"), {
      target: { value: "data" },
    });
    fireEvent.click(within(group).getByRole("option", { name: "Data" }));
    fireEvent.change(
      within(group).getByRole("spinbutton", {
        name: "Minimum officers required",
      }),
      { target: { value: "2" } },
    );
    expect(
      JSON.parse(screen.getByTestId("saved-groups").textContent || "null"),
    ).toEqual([{ officers: ["Picard", "Data"], min_count: 2 }]);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(group.closest("details")?.open).toBe(true);
  });
  it("loads existing groups, allows multiple groups, removes members and removes a group", () => {
    renderGroups('[{"officers":["Picard","Data"],"min_count":1}]');
    const first = screen.getByRole("group", { name: "Officer group 1" });
    expect(within(first).getByText("Picard")).toBeTruthy();
    fireEvent.click(within(first).getByRole("button", { name: "Remove Data" }));
    fireEvent.click(screen.getByRole("button", { name: "Add officer group" }));
    expect(screen.getByRole("group", { name: "Officer group 2" })).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Remove officer group 2" }),
    );
    expect(
      JSON.parse(screen.getByTestId("saved-groups").textContent || "null"),
    ).toEqual([{ officers: ["Picard"], min_count: 1 }]);
  });
  it("shows invalid empty groups and impossible minimum counts before submission", () => {
    renderGroups('[{"officers":["Picard"],"min_count":2}]');
    expect(screen.getByRole("alert").textContent).toMatch(
      /minimum.*selected officers/i,
    );
    fireEvent.change(
      screen.getByRole("spinbutton", { name: "Minimum officers required" }),
      { target: { value: "1" } },
    );
    expect(screen.queryByRole("alert")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Remove Picard" }));
    expect(screen.getByRole("alert").textContent).toMatch(
      /choose at least one officer/i,
    );
  });
  it("preserves saved officer names containing commas when editing a group", () => {
    renderGroups('[{"officers":["Picard, Locutus","Data"],"min_count":1}]');
    fireEvent.click(screen.getByRole("button", { name: "Remove Data" }));
    expect(
      JSON.parse(screen.getByTestId("saved-groups").textContent || "null"),
    ).toEqual([{ officers: ["Picard, Locutus"], min_count: 1 }]);
  });
  it("retains malformed saved group settings visibly until explicitly cleared", () => {
    renderGroups("{damaged settings");
    expect(screen.getByRole("alert").textContent).toMatch(
      /saved officer groups/i,
    );
    expect(screen.getByTestId("saved-groups").textContent).toBe(
      "{damaged settings",
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Clear invalid groups" }),
    );
    expect(screen.queryByRole("alert")).toBeNull();
  });
  it("uses descriptions of recommendation effects instead of storage and algorithm labels", () => {
    renderPanel({ noveltyLambdaText: "0.5" });
    fireEvent.click(screen.getByText("Recommendation variety"));
    expect(
      screen.getByRole("checkbox", {
        name: "Prefer different officers from previous recommendations",
      }),
    ).toBeTruthy();
    expect(
      screen.queryByText(/persisted optimize_history|warm-start cache key/),
    ).toBeNull();
    expect(screen.getByText("Recommendation variety")).toBeTruthy();
  });
});

describe("OptimizePanel", () => {
  it("renders expanded panel with Strategy heading", () => {
    renderPanel();
    expect(screen.getByText("Strategy")).toBeTruthy();
  });
  it("renders collapsed state with expand button", () => {
    renderPanel({}, { collapsed: true });
    expect(screen.getByLabelText("Expand panel")).toBeTruthy();
  });
  it("calls onToggleCollapsed when collapse button clicked", () => {
    const fn = vi.fn();
    renderPanel({}, { onToggleCollapsed: fn });
    fireEvent.click(screen.getByLabelText("Collapse panel"));
    expect(fn).toHaveBeenCalledOnce();
  });
  it("shows starter crews when available", () => {
    renderPanel({}, { availableSeeds: ["swarm-crews", "borg-crews"] });
    expect(screen.getByText("Starter crews")).toBeTruthy();
    expect(screen.getByText("swarm-crews")).toBeTruthy();
    expect(screen.getByText("borg-crews")).toBeTruthy();
  });
  it("shows starter crews heading and hint when no seeds", () => {
    renderPanel();
    expect(screen.getByText("Starter crews")).toBeTruthy();
    expect(
      screen.getByRole("note", { name: /data\/heuristics/i }),
    ).toBeTruthy();
    expect(
      screen.getByText(/No starter crew lists are available/i),
    ).toBeTruthy();
  });
  it("shows below-decks selection when seeds are selected", () => {
    renderPanel(
      { selectedSeeds: ["swarm-crews"] },
      { availableSeeds: ["swarm-crews"] },
    );
    expect(screen.getByText("Below-decks selection")).toBeTruthy();
    expect(screen.getByText(/Use only starter crews/)).toBeTruthy();
    expect(
      screen.getByRole("checkbox", {
        name: "Include starter crews in the main search",
      }),
    ).toBeTruthy();
  });
  it("disables fast discovery when strategy is genetic", () => {
    renderPanel(
      { selectedSeeds: ["swarm-crews"], optimizerStrategy: "genetic" },
      { availableSeeds: ["swarm-crews"] },
    );
    expect(
      (
        screen.getByRole("checkbox", {
          name: "Include starter crews in the main search",
        }) as HTMLInputElement
      ).disabled,
    ).toBe(true);
  });
  it("updates max candidates through the typed settings callback", () => {
    const fn = vi.fn();
    renderPanel({}, { onChange: fn });
    fireEvent.change(screen.getByPlaceholderText("Auto limit (10,000)"), {
      target: { value: "500" },
    });
    expect(fn).toHaveBeenCalledWith({ maxCandidates: 500 });
  });
  it("#327 describes the server safety limit when max crews is blank", () => {
    renderPanel();
    expect(screen.getByPlaceholderText("Auto limit (10,000)")).toBeTruthy();
    expect(
      screen.getByText(/Blank uses the server's 10,000-candidate safety limit/),
    ).toBeTruthy();
  });
  it("clamps max candidates to 2,000,000", () => {
    const fn = vi.fn();
    renderPanel({}, { onChange: fn });
    fireEvent.change(screen.getByPlaceholderText("Auto limit (10,000)"), {
      target: { value: "9999999" },
    });
    expect(fn).toHaveBeenCalledWith({ maxCandidates: 2_000_000 });
  });
  it("sets max candidates to null when input cleared", () => {
    const fn = vi.fn();
    renderPanel({ maxCandidates: 100 }, { onChange: fn });
    fireEvent.change(screen.getByPlaceholderText("Auto limit (10,000)"), {
      target: { value: "" },
    });
    expect(fn).toHaveBeenCalledWith({ maxCandidates: null });
  });
  it("changes the below-decks pool mode select", () => {
    const fn = vi.fn();
    renderPanel({}, { onChange: fn });
    fireEvent.click(screen.getByText("Search scope"));
    fireEvent.change(
      screen.getByRole("combobox", { name: /Below-decks officer pool/ }),
      { target: { value: "scored" } },
    );
    expect(fn).toHaveBeenCalledWith({ belowDecksPoolMode: "scored" });
  });
  it("shows live status during optimization", () => {
    renderPanel(
      {},
      {
        progress: {
          loadingOptimize: true,
          optimizeCrewsDone: 50,
          optimizeTotalCrews: 200,
        },
      },
    );
    expect(screen.getByText(/Live: units 50 \/ 200/)).toBeTruthy();
  });
  it("shows scouting fields when strategy is tiered", () => {
    renderPanel({ optimizerStrategy: "tiered" });
    expect(screen.getByPlaceholderText("500 (default)")).toBeTruthy();
    expect(screen.getByPlaceholderText("20 (default)")).toBeTruthy();
  });
  it("hides scouting fields when strategy is not tiered", () => {
    renderPanel({ optimizerStrategy: "exhaustive" });
    expect(screen.queryByPlaceholderText("500 (default)")).toBeNull();
    expect(screen.queryByPlaceholderText("20 (default)")).toBeNull();
  });
  it("toggles the successful-pairs preference", () => {
    const fn = vi.fn();
    renderPanel({}, { onChange: fn });
    fireEvent.click(
      screen.getByRole("checkbox", {
        name: /Favor officer pairs that worked before/i,
      }),
    );
    expect(fn).toHaveBeenCalledWith({ enableLearnedPairPrior: false });
  });
  it("offers the refine-finalists toggle on the lanes that support it", () => {
    for (const optimizerStrategy of ["tiered", "genetic"] as const) {
      const view = renderPanel({ optimizerStrategy });
      expect(
        screen.getByRole("checkbox", { name: /Refine finalists/i }),
      ).toBeTruthy();
      view.unmount();
    }
  });
  it("hides the refine-finalists toggle where the server ignores it", () => {
    for (const optimizerStrategy of ["exhaustive", "linear_eval"] as const) {
      const view = renderPanel({ optimizerStrategy });
      expect(
        screen.queryByRole("checkbox", { name: /Refine finalists/i }),
      ).toBeNull();
      view.unmount();
    }
  });
  it("toggles refine finalists", () => {
    const fn = vi.fn();
    renderPanel({ optimizerStrategy: "tiered" }, { onChange: fn });
    fireEvent.click(
      screen.getByRole("checkbox", { name: /Refine finalists/i }),
    );
    expect(fn).toHaveBeenCalledWith({ localRefinement: true });
  });
  it("updates scout sims and clamps to 100,000", () => {
    const fn = vi.fn();
    renderPanel({ optimizerStrategy: "tiered" }, { onChange: fn });
    const input = screen.getByPlaceholderText("500 (default)");
    fireEvent.change(input, { target: { value: "2000" } });
    expect(fn).toHaveBeenCalledWith({ tieredScoutSims: 2000 });
    fireEvent.change(input, { target: { value: "999999" } });
    expect(fn).toHaveBeenCalledWith({ tieredScoutSims: 100_000 });
  });
  it("clears scout sims when input emptied", () => {
    const fn = vi.fn();
    renderPanel({ tieredScoutSims: 100 }, { onChange: fn });
    fireEvent.change(screen.getByPlaceholderText("500 (default)"), {
      target: { value: "" },
    });
    expect(fn).toHaveBeenCalledWith({ tieredScoutSims: null });
  });
  it("shows saved results badge when last run reused profile history", () => {
    renderPanel(
      {},
      { progress: { ...baseProps.progress, cachedWarmStartBadge: true } },
    );
    expect(screen.getByText("Saved results reused")).toBeTruthy();
  });
  it("says why saved results were ignored and names the cause", () => {
    renderPanel(
      {},
      {
        progress: { ...baseProps.progress, staleCacheRefusedReason: "profile" },
      },
    );
    expect(screen.getByText("Saved results ignored")).toBeTruthy();
    expect(screen.getByText(/Your profile changed/i)).toBeTruthy();
    expect(screen.getByText(/re-simulated/i)).toBeTruthy();
  });
  it("hides the ignored-results notice when nothing was refused", () => {
    renderPanel();
    expect(screen.queryByText("Saved results ignored")).toBeNull();
  });
  it("shows approximate ranking and hides incompatible fields", () => {
    renderPanel({ optimizerStrategy: "linear_eval" });
    expect(
      screen.getByRole("option", { name: "Estimate expected damage" }),
    ).toBeTruthy();
    expect(screen.queryByText("Quick trials per crew (optional)")).toBeNull();
    expect(screen.queryByText(/Chain grind/)).toBeNull();
    expect(
      screen.queryByText("Favor officer pairs that worked before"),
    ).toBeNull();
  });
});
