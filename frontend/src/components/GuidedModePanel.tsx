import { useEffect, useState } from "react";

const STEPS = ["Scenario", "Crew", "Run", "Results"] as const;

interface GuidedModePanelProps {
  shipSelected: boolean;
  targetSelected: boolean;
  crewReady: boolean;
  running: boolean;
  hasResults: boolean;
  onRunSim: () => void;
  onRunOptimize: () => void;
  onExit: () => void;
}

function scrollTo(id: string) {
  document
    .getElementById(id)
    ?.scrollIntoView({ behavior: "smooth", block: "center" });
}

export default function GuidedModePanel({
  shipSelected,
  targetSelected,
  crewReady,
  running,
  hasResults,
  onRunSim,
  onRunOptimize,
  onExit,
}: GuidedModePanelProps) {
  const [step, setStep] = useState(0);
  const scenarioReady = shipSelected && targetSelected;

  useEffect(() => {
    if (step === 3 && !running) scrollTo("guided-results");
  }, [step, running]);

  const go = (next: number, anchor: string) => {
    setStep(next);
    requestAnimationFrame(() => scrollTo(anchor));
  };

  return (
    <section
      aria-label="Guided mode"
      style={{
        padding: "0.8rem 1rem",
        background: "rgba(232, 149, 46, 0.1)",
        borderBottom: "1px solid var(--accent-dim)",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          flexWrap: "wrap",
        }}
      >
        <strong style={{ marginRight: 4 }}>Guided mission</strong>
        {STEPS.map((label, index) => (
          <button
            key={label}
            type="button"
            onClick={() =>
              go(
                index,
                index === 0
                  ? "guided-scenario"
                  : index === 1
                    ? "guided-crew"
                    : "guided-results",
              )
            }
            aria-current={step === index ? "step" : undefined}
            style={{
              padding: "0.25rem 0.55rem",
              borderRadius: 999,
              border: "1px solid var(--border)",
              color: step === index ? "var(--bg)" : "var(--text-muted)",
              background: step === index ? "var(--accent)" : "var(--surface)",
              fontSize: "0.78rem",
            }}
          >
            {index + 1}. {label}
          </button>
        ))}
        <button type="button" onClick={onExit} style={{ marginLeft: "auto" }}>
          Exit guided mode
        </button>
      </div>

      <div
        style={{
          marginTop: 8,
          display: "flex",
          alignItems: "center",
          gap: 10,
          flexWrap: "wrap",
        }}
      >
        {step === 0 && (
          <>
            <span>Choose your ship and the hostile you want to fight.</span>
            <button
              type="button"
              disabled={!scenarioReady}
              onClick={() => go(1, "guided-crew")}
            >
              Continue to crew
            </button>
            {!scenarioReady && <small>Select both fields to continue.</small>}
          </>
        )}
        {step === 1 && (
          <>
            <span>
              Build a bridge crew to test, or let Kobayashi search your roster.
            </span>
            <button type="button" onClick={() => go(2, "guided-results")}>
              Find a crew for me
            </button>
            <button
              type="button"
              disabled={!crewReady}
              onClick={() => go(2, "guided-results")}
            >
              Test this crew
            </button>
            {!crewReady && (
              <small>Testing requires a captain and both bridge seats.</small>
            )}
          </>
        )}
        {step === 2 && (
          <>
            <span>
              {crewReady
                ? "Run your crew or search for stronger options."
                : "Search your roster for the best available crew."}
            </span>
            {crewReady && (
              <button
                type="button"
                disabled={running || !scenarioReady}
                onClick={() => {
                  onRunSim();
                  go(3, "guided-results");
                }}
              >
                Run simulation
              </button>
            )}
            <button
              type="button"
              disabled={running || !scenarioReady}
              onClick={() => {
                onRunOptimize();
                go(3, "guided-results");
              }}
            >
              Find best crew
            </button>
          </>
        )}
        {step === 3 && (
          <>
            <span>
              {running
                ? "Kobayashi is evaluating the fight…"
                : hasResults
                  ? "Review the result below. You can change the scenario and run again."
                  : "No result yet. Go back to Run when you are ready."}
            </span>
            <button
              type="button"
              onClick={() => go(0, "guided-scenario")}
              disabled={running}
            >
              Start another mission
            </button>
          </>
        )}
      </div>
    </section>
  );
}
