import { useEffect } from "react";

const STEPS = ["Scenario", "Crew", "Run", "Results"] as const;
export type GuidedStep = 0 | 1 | 2 | 3;

interface GuidedModePanelProps {
  step: GuidedStep;
  onStepChange: (step: GuidedStep) => void;
  shipSelected: boolean;
  targetSelected: boolean;
  crewReady: boolean;
  running: boolean;
  optimizing?: boolean;
  hasResults: boolean;
  onRunSim: () => void;
  onRunOptimize: () => void;
  onCancelOptimize?: () => void;
  onRestart?: () => void;
  onExit: () => void;
}

function scrollTo(id: string) {
  document
    .getElementById(id)
    ?.scrollIntoView({ behavior: "smooth", block: "start" });
}

function anchorForStep(step: GuidedStep): string {
  if (step === 0) return "guided-scenario";
  if (step === 1) return "guided-crew";
  if (step === 2) return "guided-run";
  return "guided-results";
}

export default function GuidedModePanel({
  step,
  onStepChange,
  shipSelected,
  targetSelected,
  crewReady,
  running,
  optimizing = false,
  hasResults,
  onRunSim,
  onRunOptimize,
  onCancelOptimize,
  onRestart,
  onExit,
}: GuidedModePanelProps) {
  const scenarioReady = shipSelected && targetSelected;

  useEffect(() => {
    requestAnimationFrame(() => scrollTo(anchorForStep(step)));
  }, [step]);

  const go = (next: GuidedStep) => {
    if (next === 0 && step !== 0) onRestart?.();
    onStepChange(next);
  };
  const stepDisabled = (index: GuidedStep) => {
    if (index === 0) return false;
    if (index === 1 || index === 2) return !scenarioReady;
    return index !== step && !running && !hasResults;
  };

  return (
    <section
      aria-label="Guided mode"
      style={{
        padding: "1rem",
        background: "rgba(232, 149, 46, 0.08)",
        borderBottom: "1px solid var(--accent-dim)",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          flexWrap: "wrap",
          maxWidth: 1120,
          margin: "0 auto",
        }}
      >
        <strong style={{ marginRight: 8 }}>Guided mission</strong>
        {STEPS.map((label, index) => {
          const guidedIndex = index as GuidedStep;
          const disabled = stepDisabled(guidedIndex);
          return (
            <button
              key={label}
              type="button"
              onClick={() => go(guidedIndex)}
              disabled={disabled}
              aria-current={step === index ? "step" : undefined}
              style={{
                padding: "0.3rem 0.65rem",
                borderRadius: 999,
                border: "1px solid var(--border)",
                color: step === index ? "var(--bg)" : "var(--text-muted)",
                background: step === index ? "var(--accent)" : "var(--surface)",
                fontSize: "0.78rem",
                opacity: disabled ? 0.45 : 1,
                cursor: disabled ? "not-allowed" : "pointer",
              }}
            >
              {index + 1}. {label}
            </button>
          );
        })}
        <button type="button" onClick={onExit} style={{ marginLeft: "auto" }}>
          Exit guided mode
        </button>
      </div>

      <div
        id={step === 2 ? "guided-run" : undefined}
        style={{
          maxWidth: 1120,
          margin: "0.9rem auto 0",
          padding: "0.9rem 1rem",
          background: "var(--surface)",
          border: "1px solid var(--border)",
          borderRadius: 8,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 12,
          flexWrap: "wrap",
        }}
      >
        {step === 0 && (
          <>
            <div>
              <strong>Choose the fight</strong>
              <div style={{ color: "var(--text-muted)", fontSize: "0.86rem" }}>
                Select your ship and the hostile you want to face below.
              </div>
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              {!scenarioReady && <small>Select both fields to continue.</small>}
              <button
                type="button"
                disabled={!scenarioReady}
                onClick={() => go(1)}
              >
                Continue to crew
              </button>
            </div>
          </>
        )}

        {step === 1 && (
          <>
            <div>
              <strong>Choose how to crew the ship</strong>
              <div style={{ color: "var(--text-muted)", fontSize: "0.86rem" }}>
                Build a bridge crew below, or skip crew selection and let
                Kobayashi search your roster.
              </div>
            </div>
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              <button type="button" onClick={() => go(0)}>
                Back
              </button>
              <button type="button" onClick={() => go(2)}>
                Find a crew for me
              </button>
              <button type="button" disabled={!crewReady} onClick={() => go(2)}>
                Test this crew
              </button>
            </div>
          </>
        )}

        {step === 2 && (
          <>
            <div>
              <strong>{crewReady ? "Ready to run" : "Ready to search"}</strong>
              <div style={{ color: "var(--text-muted)", fontSize: "0.86rem" }}>
                {crewReady
                  ? "Test your selected crew, or search your roster for stronger options."
                  : "Kobayashi will use the default tiered search to find the strongest available crews."}
              </div>
            </div>
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              <button type="button" onClick={() => go(1)} disabled={running}>
                Back
              </button>
              {crewReady && (
                <button
                  type="button"
                  disabled={running || !scenarioReady}
                  onClick={() => {
                    onRunSim();
                    go(3);
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
                  go(3);
                }}
              >
                Find best crew
              </button>
            </div>
          </>
        )}

        {step === 3 && (
          <>
            <div>
              <strong>{running ? "Mission running" : "Mission results"}</strong>
              <div style={{ color: "var(--text-muted)", fontSize: "0.86rem" }}>
                {running
                  ? "Kobayashi is evaluating the fight. Progress appears below."
                  : hasResults
                    ? "Review the result below, then start another mission or revisit an earlier step."
                    : "The run did not produce a result. Return to Run and try again."}
              </div>
            </div>
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              {optimizing && onCancelOptimize && (
                <button type="button" onClick={onCancelOptimize}>
                  Cancel run
                </button>
              )}
              {!running && !hasResults && (
                <button type="button" onClick={() => go(2)}>
                  Back to run
                </button>
              )}
              <button type="button" onClick={() => go(0)} disabled={running}>
                Start another mission
              </button>
            </div>
          </>
        )}
      </div>
    </section>
  );
}
