import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import GuidedModePanel, { type GuidedStep } from "./GuidedModePanel";

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

function GuidedHarness({
  targetSelected = true,
  crewReady = false,
  running = false,
  hasResults = false,
  onRunOptimize = vi.fn(),
}: {
  targetSelected?: boolean;
  crewReady?: boolean;
  running?: boolean;
  hasResults?: boolean;
  onRunOptimize?: () => void;
}) {
  const [step, setStep] = useState<GuidedStep>(0);
  return (
    <GuidedModePanel
      {...baseProps}
      step={step}
      onStepChange={setStep}
      targetSelected={targetSelected}
      crewReady={crewReady}
      running={running}
      hasResults={hasResults}
      onRunOptimize={onRunOptimize}
    />
  );
}

describe("GuidedModePanel", () => {
  it("walks from scenario to an optimizer run", () => {
    const onRunOptimize = vi.fn();
    render(<GuidedHarness onRunOptimize={onRunOptimize} />);

    fireEvent.click(screen.getByRole("button", { name: "Continue to crew" }));
    fireEvent.click(screen.getByRole("button", { name: "Find a crew for me" }));
    fireEvent.click(screen.getByRole("button", { name: "Find best crew" }));

    expect(onRunOptimize).toHaveBeenCalledOnce();
    expect(screen.getByText("Mission results")).toBeTruthy();
    expect(
      screen.getByText(
        "The run did not produce a result. Return to Run and try again.",
      ),
    ).toBeTruthy();
  });

  it("requires a complete scenario before continuing", () => {
    render(<GuidedHarness targetSelected={false} />);
    expect(
      screen.getByRole("button", { name: "Continue to crew" }),
    ).toHaveProperty("disabled", true);
    expect(screen.getByRole("button", { name: "2. Crew" })).toHaveProperty(
      "disabled",
      true,
    );
  });

  it("only offers a direct simulation for a complete bridge crew", () => {
    render(<GuidedHarness crewReady={true} />);
    fireEvent.click(screen.getByRole("button", { name: "Continue to crew" }));
    fireEvent.click(screen.getByRole("button", { name: "Test this crew" }));

    expect(screen.getByRole("button", { name: "Run simulation" })).toBeTruthy();
  });
});
