import { formatOptimizePhaseLabel } from "../../lib/api";

/** Plain-language reason saved confirmations could not be trusted for this run. */
function staleCacheRefusedReasonText(reason: string): string {
  switch (reason) {
    case "engine":
      return "The combat engine changed since these results were saved.";
    case "data":
      return "The game data catalog changed since these results were saved.";
    case "profile":
      return "Your profile changed since these results were saved.";
    case "scenario":
      return "This ship or target resolves differently than when these results were saved.";
    case "unfingerprinted":
      return "These saved results predate result fingerprinting, so they cannot be verified.";
    default:
      return "Saved results no longer match this run.";
  }
}

export interface OptimizerProgress {
  loadingOptimize: boolean;
  optimizeCrewsDone: number | null;
  optimizeTotalCrews: number | null;
  optimizePhase?: string | null;
  optimizeEtaSeconds?: number | null;
  optimizeThroughput?: number | null;
  cachedWarmStartBadge?: boolean;
  staleCacheRefusedReason?: string | null;
}
export default function ProgressSection({
  loadingOptimize,
  optimizeCrewsDone,
  optimizeTotalCrews,
  optimizePhase,
  optimizeEtaSeconds,
  optimizeThroughput,
  cachedWarmStartBadge,
  staleCacheRefusedReason,
}: OptimizerProgress) {
  return (
    <>
      <p style={{ margin: 0, fontSize: "0.8rem", color: "var(--text-muted)" }}>
        {loadingOptimize &&
        optimizeCrewsDone != null &&
        optimizeTotalCrews != null &&
        optimizeTotalCrews > 0
          ? `Live: ${optimizePhase === "genetic" ? "gen" : "units"} ${optimizeCrewsDone} / ${optimizeTotalCrews}${
              formatOptimizePhaseLabel(optimizePhase)
                ? ` · ${formatOptimizePhaseLabel(optimizePhase)}`
                : ""
            }${
              optimizeThroughput != null && optimizePhase !== "genetic"
                ? ` · ~${optimizeThroughput.toFixed(1)}/s`
                : ""
            }${optimizeEtaSeconds != null ? ` · ETA ~${optimizeEtaSeconds}s` : ""}`
          : "Live status: — (run optimize to see phase, ETA, preview)"}
      </p>
      {cachedWarmStartBadge && !loadingOptimize ? (
        <p
          style={{
            margin: "6px 0 0",
            fontSize: "0.78rem",
            color: "var(--text-muted)",
          }}
        >
          <span
            style={{
              display: "inline-block",
              padding: "2px 8px",
              borderRadius: 999,
              border: "1px solid var(--border)",
              background: "var(--bg-elevated, var(--bg))",
              fontWeight: 600,
            }}
          >
            Saved results reused
          </span>
          <span style={{ marginLeft: 8 }}>
            Previously verified crews were reused from this profile.
          </span>
        </p>
      ) : null}
      {staleCacheRefusedReason && !loadingOptimize ? (
        <p
          style={{
            margin: "6px 0 0",
            fontSize: "0.78rem",
            color: "var(--text-muted)",
          }}
        >
          <span
            style={{
              display: "inline-block",
              padding: "2px 8px",
              borderRadius: 999,
              border: "1px solid var(--border)",
              background: "var(--bg-elevated, var(--bg))",
              fontWeight: 600,
            }}
          >
            Saved results ignored
          </span>
          <span style={{ marginLeft: 8 }}>
            {staleCacheRefusedReasonText(staleCacheRefusedReason)} Every crew
            was re-simulated, so these numbers are fresh.
          </span>
        </p>
      ) : null}
    </>
  );
}
