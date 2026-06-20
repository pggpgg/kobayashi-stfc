import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import GuidedModePanel from "./GuidedModePanel";

const baseProps = {
  shipSelected: true,
  targetSelected: true,
  crewReady: false,
  running: false,
  hasResults: false,
  onRunSim: vi.fn(),
  onRunOptimize: vi.fn(),
  onExit: vi.fn(),
};

describe("GuidedModePanel", () => {
  it("walks from scenario to an optimizer run", () => {
    const onRunOptimize = vi.fn();
    render(<GuidedModePanel {...baseProps} onRunOptimize={onRunOptimize} />);

    fireEvent.click(screen.getByRole("button", { name: "Continue to crew" }));
    fireEvent.click(screen.getByRole("button", { name: "Find a crew for me" }));
    fireEvent.click(screen.getByRole("button", { name: "Find best crew" }));

    expect(onRunOptimize).toHaveBeenCalledOnce();
    expect(
      screen.getByText("No result yet. Go back to Run when you are ready."),
    ).toBeTruthy();
  });

  it("requires a complete scenario before continuing", () => {
    render(<GuidedModePanel {...baseProps} targetSelected={false} />);
    expect(
      screen.getByRole("button", { name: "Continue to crew" }),
    ).toHaveProperty("disabled", true);
  });
});
