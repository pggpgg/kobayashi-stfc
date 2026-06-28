import {
  type CSSProperties,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import HostilePicker from "../components/HostilePicker";
import MorrisResults from "../components/MorrisResults";
import SensitivityResults from "../components/SensitivityResults";
import SobolResults from "../components/SobolResults";
import { useProfile } from "../contexts/ProfileContext";
import {
  ApiError,
  type CpuBusyWaitInfo,
  fetchHostiles,
  fetchShips,
  type HostileListItem,
  type ShipListItem,
} from "../lib/api";
import {
  cancelSensitivityJob,
  fetchMorrisDefaults,
  fetchSensitivityDefaults,
  fetchSobolDefaults,
  formatSensitivityPhaseLabel,
  getSensitivityStreamUrl,
  type MorrisResponse,
  type OutcomeMetric,
  type SensitivityDefaultRow,
  type SensitivityResponse,
  type SensitivityStatusResponse,
  type SobolResponse,
  sensitivityStart,
} from "../lib/sensitivityApi";

/** Repeated style objects, hoisted so each is defined once (behavior-preserving). */
const styles = {
  mutedSm: { fontSize: "0.85rem", color: "var(--text-muted)" },
  col: { display: "flex", flexDirection: "column", gap: 4 },
  sectionTop: { marginTop: "1.5rem" },
  mutedXs: { fontSize: "0.75rem", color: "var(--text-muted)" },
  cellCenter: { textAlign: "center", padding: "0.35rem 0.5rem" },
  mb05: { marginBottom: "0.5rem" },
  cellRight: { textAlign: "right", padding: "0.35rem 0.5rem" },
} satisfies Record<string, CSSProperties>;

type AnalysisMethod = "oat" | "morris" | "sobol";

const METRICS: { value: OutcomeMetric; label: string; hint: string }[] = [
  {
    value: "hull_remaining",
    label: "Attacker hull remaining",
    hint: "Continuous in [0,1]. PvE default.",
  },
  {
    value: "defender_hull_remaining",
    label: "Defender hull destroyed (1 − remaining)",
    hint: "Use when fights don't end (e.g. damage-sponge hostiles).",
  },
  {
    value: "win_rate",
    label: "Win rate (1.0 / 0.0)",
    hint: "Best for PvP — binary outcome.",
  },
  {
    value: "rounds_to_kill",
    label: "Rounds to kill (negated)",
    hint: "Lower rounds = better; sign is flipped so larger Δ is always better.",
  },
];

interface DeltaRowState {
  stat: string;
  delta: number;
  multiplicative: boolean;
  enabled: boolean;
}

export default function Sensitivity() {
  const { activeProfileId } = useProfile();
  const [ships, setShips] = useState<ShipListItem[]>([]);
  const [hostiles, setHostiles] = useState<HostileListItem[]>([]);
  const [shipId, setShipId] = useState<string>("");
  const [scenarioId, setScenarioId] = useState<string>("");
  const [shipTier, setShipTier] = useState<number>(5);
  const [shipLevel, setShipLevel] = useState<number>(1);
  const [captain, setCaptain] = useState<string>("");
  const [bridgeText, setBridgeText] = useState<string>("");
  const [belowDecksText, setBelowDecksText] = useState<string>("");
  const [metric, setMetric] = useState<OutcomeMetric>("hull_remaining");
  const [numSims, setNumSims] = useState<number>(2000);
  const [seed, setSeed] = useState<number>(0);
  const [defaults, setDefaults] = useState<DeltaRowState[]>([]);
  const [running, setRunning] = useState<boolean>(false);
  const [response, setResponse] = useState<SensitivityResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [method, setMethod] = useState<AnalysisMethod>("oat");
  const [morrisNumSims, setMorrisNumSims] = useState<number>(200);
  const [rTrajectories, setRTrajectories] = useState<number>(10);
  const [morrisResponse, setMorrisResponse] = useState<MorrisResponse | null>(
    null,
  );
  const [sobolNSamples, setSobolNSamples] = useState<number>(512);
  const [sobolIncludePairwise, setSobolIncludePairwise] =
    useState<boolean>(false);
  const [sobolResponse, setSobolResponse] = useState<SobolResponse | null>(
    null,
  );

  // Async-job state: jobId is set as soon as the server replies to POST .../start;
  // jobProgress / phase / sims / throughput / eta are updated by SSE events on every
  // ~300ms tick. cpuBusyInfo is shown in a banner when fetchWithCpuBusyRetries waits.
  const [jobId, setJobId] = useState<string | null>(null);
  const [jobProgress, setJobProgress] = useState<number | null>(null);
  const [jobPhase, setJobPhase] = useState<string | null>(null);
  const [jobSimsDone, setJobSimsDone] = useState<number | null>(null);
  const [jobTotalSims, setJobTotalSims] = useState<number | null>(null);
  const [jobThroughput, setJobThroughput] = useState<number | null>(null);
  const [jobEtaSeconds, setJobEtaSeconds] = useState<number | null>(null);
  const [cpuBusyInfo, setCpuBusyInfo] = useState<CpuBusyWaitInfo | null>(null);
  const eventSourceRef = useRef<EventSource | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchShips(false, activeProfileId).then((list) => {
      if (!cancelled) setShips(list);
    });
    fetchHostiles().then((list) => {
      if (!cancelled) setHostiles(list);
    });
    fetchSensitivityDefaults()
      .then((resp) => {
        if (cancelled) return;
        setDefaults(
          resp.deltas.map((d: SensitivityDefaultRow) => ({
            stat: d.stat,
            delta: d.delta,
            multiplicative: d.multiplicative,
            enabled: true,
          })),
        );
      })
      .catch((e: unknown) => {
        if (!cancelled)
          setError(
            e instanceof Error
              ? `Failed to load default deltas: ${e.message}`
              : "Failed to load default deltas",
          );
      });
    fetchMorrisDefaults()
      .then((resp) => {
        if (cancelled) return;
        setMorrisNumSims(resp.num_sims_default);
        setRTrajectories(resp.r_trajectories_default);
      })
      .catch(() => {
        // Non-fatal: server defaults will be applied if user submits without changing.
      });
    fetchSobolDefaults()
      .then((resp) => {
        if (cancelled) return;
        setSobolNSamples(resp.n_samples_default);
      })
      .catch(() => {
        // Non-fatal.
      });
    return () => {
      cancelled = true;
    };
  }, [activeProfileId]);

  const parsedBridge = useMemo(
    () =>
      bridgeText
        .split(",")
        .map((s) => s.trim())
        .filter((s) => s.length > 0),
    [bridgeText],
  );
  const parsedBelowDecks = useMemo(
    () =>
      belowDecksText
        .split(",")
        .map((s) => s.trim())
        .filter((s) => s.length > 0),
    [belowDecksText],
  );

  const canRun = shipId !== "" && scenarioId !== "";

  /** Reset all live-progress state. Called when starting a new run or after a job ends. */
  function resetJobState() {
    setJobId(null);
    setJobProgress(null);
    setJobPhase(null);
    setJobSimsDone(null);
    setJobTotalSims(null);
    setJobThroughput(null);
    setJobEtaSeconds(null);
    setCpuBusyInfo(null);
  }

  /** Apply one SSE status snapshot to the live-progress state. */
  function applyStatus(status: SensitivityStatusResponse) {
    setJobProgress(status.progress ?? null);
    setJobPhase(status.phase ?? null);
    setJobSimsDone(status.sims_done ?? null);
    setJobTotalSims(status.total_sims ?? null);
    setJobThroughput(status.throughput_sims_per_sec ?? null);
    setJobEtaSeconds(status.eta_seconds ?? null);
  }

  async function handleRun() {
    if (!canRun) return;
    setRunning(true);
    setError(null);
    setResponse(null);
    setMorrisResponse(null);
    setSobolResponse(null);
    resetJobState();
    try {
      const deltasMap: Record<string, number> = {};
      for (const row of defaults) {
        if (!row.enabled) {
          deltasMap[row.stat] = 0;
        } else {
          deltasMap[row.stat] = row.delta;
        }
      }
      const sharedScenario = {
        ship: shipId,
        hostile: scenarioId,
        ship_tier: shipTier,
        ship_level: shipLevel,
        captain: captain || undefined,
        bridge: parsedBridge,
        below_decks: parsedBelowDecks,
        metric,
        seed,
        deltas: deltasMap,
        profile_id: activeProfileId ?? undefined,
      };

      const requestBody =
        method === "oat"
          ? { ...sharedScenario, num_sims: numSims }
          : method === "morris"
            ? {
                ...sharedScenario,
                num_sims: morrisNumSims,
                r_trajectories: rTrajectories,
              }
            : {
                ...sharedScenario,
                n_samples: sobolNSamples,
                include_pairwise: sobolIncludePairwise,
              };

      const startResp = await sensitivityStart(
        method,
        requestBody,
        activeProfileId,
        {
          onCpuBusyWait: (info) => setCpuBusyInfo(info),
        },
      );
      setCpuBusyInfo(null);
      setJobId(startResp.job_id);

      // Subscribe to the SSE stream. The server emits one status snapshot every
      // ~300ms until the job hits done / error; the final event carries the full
      // result payload. Fallback: if the stream errors out, we let the UI show the
      // error so the user can retry — no polling fallback wired here (different
      // tradeoff from optimize, where the page is much longer-running).
      const url = getSensitivityStreamUrl(startResp.job_id);
      const es = new EventSource(url);
      eventSourceRef.current = es;
      es.onmessage = (event) => {
        try {
          const status = JSON.parse(event.data) as SensitivityStatusResponse;
          applyStatus(status);
          if (status.status === "done" && status.result) {
            if (status.method === "oat") {
              setResponse(status.result as unknown as SensitivityResponse);
            } else if (status.method === "morris") {
              setMorrisResponse(status.result as unknown as MorrisResponse);
            } else if (status.method === "sobol") {
              setSobolResponse(status.result as unknown as SobolResponse);
            }
            es.close();
            eventSourceRef.current = null;
            setRunning(false);
          } else if (status.status === "error") {
            setError(status.error ?? "Job failed");
            es.close();
            eventSourceRef.current = null;
            setRunning(false);
          }
        } catch (parseErr) {
          // Ignore malformed SSE frames; the next tick should recover.
          console.warn("sensitivity SSE parse error", parseErr);
        }
      };
      es.onerror = () => {
        // EventSource auto-retries on transient errors; if the connection is
        // permanently broken the next status snapshot won't arrive and the user
        // can cancel. Surface a soft hint in the phase label.
        setJobPhase("(reconnecting…)");
      };
    } catch (e: unknown) {
      if (e instanceof ApiError) {
        setError(`${e.code}: ${e.message}`);
      } else if (e instanceof Error) {
        setError(e.message);
      } else {
        setError("Unknown error");
      }
      setRunning(false);
    }
  }

  async function handleCancel() {
    if (!jobId) return;
    try {
      await cancelSensitivityJob(jobId);
    } catch (e) {
      console.warn("cancel sensitivity job failed", e);
    }
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
    }
    setRunning(false);
    setError("Cancelled");
  }

  // Cleanup: close any open SSE on unmount.
  useEffect(() => {
    return () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
        eventSourceRef.current = null;
      }
    };
  }, []);

  return (
    <div style={{ padding: "1.5rem", maxWidth: 1100 }}>
      <h2 style={{ marginTop: 0 }}>Sensitivity analysis</h2>
      <p style={{ color: "var(--text-muted)", maxWidth: 800 }}>
        For a fixed scenario, perturb each in-game stat by one realistic step of
        investment and measure how the outcome changes. Stats with a 95% CI that
        crosses zero are dimmed — they don't move the needle for this scenario
        at the chosen N.
      </p>

      <section style={{ marginTop: "1rem" }}>
        <h3 style={styles.mb05}>Scenario</h3>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
            gap: "0.75rem",
          }}
        >
          <label style={styles.col}>
            <span style={styles.mutedSm}>Ship</span>
            <select value={shipId} onChange={(e) => setShipId(e.target.value)}>
              <option value="">— pick a ship —</option>
              {ships.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.ship_name}
                </option>
              ))}
            </select>
          </label>
          <div style={styles.col}>
            <span style={styles.mutedSm}>Hostile</span>
            <HostilePicker
              hostiles={hostiles}
              value={scenarioId}
              onChange={setScenarioId}
            />
          </div>
          <label style={styles.col}>
            <span style={styles.mutedSm}>Ship tier</span>
            <input
              type="number"
              min={1}
              max={15}
              value={shipTier}
              onChange={(e) => setShipTier(Number(e.target.value))}
            />
          </label>
          <label style={styles.col}>
            <span style={styles.mutedSm}>Ship level</span>
            <input
              type="number"
              min={1}
              max={99}
              value={shipLevel}
              onChange={(e) => setShipLevel(Number(e.target.value))}
            />
          </label>
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 2fr 2fr",
            gap: "0.75rem",
            marginTop: "0.75rem",
          }}
        >
          <label style={styles.col}>
            <span style={styles.mutedSm}>Captain id</span>
            <input
              type="text"
              value={captain}
              onChange={(e) => setCaptain(e.target.value)}
              placeholder="e.g. ent-e-picard-556227"
            />
          </label>
          <label style={styles.col}>
            <span style={styles.mutedSm}>
              Bridge officer ids (comma-separated)
            </span>
            <input
              type="text"
              value={bridgeText}
              onChange={(e) => setBridgeText(e.target.value)}
              placeholder="ent-e-data-871245, five-of-eleven-d9aa11"
            />
          </label>
          <label style={styles.col}>
            <span style={styles.mutedSm}>
              Below decks ids (comma-separated, optional)
            </span>
            <input
              type="text"
              value={belowDecksText}
              onChange={(e) => setBelowDecksText(e.target.value)}
            />
          </label>
        </div>
      </section>

      <section style={styles.sectionTop}>
        <h3 style={styles.mb05}>Run parameters</h3>
        <div style={{ marginBottom: "0.75rem" }}>
          <label style={styles.mutedSm} htmlFor="sensitivity-method">
            Method:&nbsp;
          </label>
          {(["oat", "morris", "sobol"] as const).map((m) => (
            <label
              key={m}
              style={{
                marginRight: "1rem",
                fontSize: "0.9rem",
                cursor: "pointer",
              }}
            >
              <input
                type="radio"
                name="sensitivity-method"
                value={m}
                checked={method === m}
                onChange={() => setMethod(m)}
                style={{ marginRight: 4 }}
              />
              {m === "oat"
                ? "OAT (per-stat Δ vs baseline)"
                : m === "morris"
                  ? "Morris screening (μ*/σ across trajectories)"
                  : "Sobol variance decomposition (S₁ / S_T)"}
            </label>
          ))}
          <p
            style={{
              marginTop: "0.4rem",
              marginBottom: 0,
              fontSize: "0.78rem",
              color: "var(--text-muted)",
            }}
          >
            {method === "oat"
              ? "Perturb one stat at a time from the same baseline. Tight CI per stat; doesn't capture interactions."
              : method === "morris"
                ? "Walk r random trajectories through stat space. μ* ranks importance; σ flags stats that interact with others."
                : "Decompose Var(Y) via Saltelli design. S₁ = main effect alone; S_T = main + all interactions. Most rigorous; cost is N × (k+2) sims."}
          </p>
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))",
            gap: "0.75rem",
          }}
        >
          <label style={styles.col}>
            <span style={styles.mutedSm}>Metric</span>
            <select
              value={metric}
              onChange={(e) => setMetric(e.target.value as OutcomeMetric)}
            >
              {METRICS.map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label}
                </option>
              ))}
            </select>
            <span style={styles.mutedXs}>
              {METRICS.find((m) => m.value === metric)?.hint}
            </span>
          </label>
          {method === "oat" && (
            <label style={styles.col}>
              <span style={styles.mutedSm}>Paired sims per stat</span>
              <input
                type="number"
                min={50}
                max={50000}
                value={numSims}
                onChange={(e) => setNumSims(Number(e.target.value))}
              />
            </label>
          )}
          {method === "morris" && (
            <>
              <label style={styles.col}>
                <span style={styles.mutedSm}>r trajectories</span>
                <input
                  type="number"
                  min={2}
                  max={200}
                  value={rTrajectories}
                  onChange={(e) => setRTrajectories(Number(e.target.value))}
                />
                <span style={styles.mutedXs}>
                  10 is a conservative default; 20–50 tightens μ* / σ at
                  proportional cost.
                </span>
              </label>
              <label style={styles.col}>
                <span style={styles.mutedSm}>Sims per trajectory point</span>
                <input
                  type="number"
                  min={2}
                  max={10000}
                  value={morrisNumSims}
                  onChange={(e) => setMorrisNumSims(Number(e.target.value))}
                />
                <span style={styles.mutedXs}>
                  Total sims ≈ r × (k+1) × this.
                </span>
              </label>
            </>
          )}
          {method === "sobol" && (
            <>
              <label style={styles.col}>
                <span style={styles.mutedSm}>
                  N samples per Saltelli matrix
                </span>
                <input
                  type="number"
                  min={8}
                  max={8192}
                  value={sobolNSamples}
                  onChange={(e) => setSobolNSamples(Number(e.target.value))}
                />
                <span style={styles.mutedXs}>
                  Total sims = N × (k + 2). N=512 is a reasonable default; raise
                  to 2048 for tighter CIs.
                </span>
              </label>
              <label
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: 4,
                  alignSelf: "flex-end",
                }}
              >
                <span
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 6,
                    fontSize: "0.88rem",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={sobolIncludePairwise}
                    onChange={(e) => setSobolIncludePairwise(e.target.checked)}
                  />
                  Also compute pairwise interactions (S
                  <sub>ij</sub>)
                </span>
                <span style={styles.mutedXs}>
                  Adds N × k(k−1)/2 extra sims (≈ 8× more at defaults). Reveals
                  which specific pairs of stats produce value together.
                </span>
              </label>
            </>
          )}
          <label style={styles.col}>
            <span style={styles.mutedSm}>Base seed</span>
            <input
              type="number"
              min={0}
              value={seed}
              onChange={(e) => setSeed(Number(e.target.value))}
            />
          </label>
        </div>
      </section>

      <section style={styles.sectionTop}>
        <h3 style={styles.mb05}>Per-stat overrides</h3>
        <p
          style={{
            fontSize: "0.85rem",
            color: "var(--text-muted)",
            marginTop: 0,
          }}
        >
          Defaults are pre-filled from the server. Edit any δ value or uncheck a
          row to skip that stat. <em>Multiplicative</em> stats apply as{" "}
          <code>value × (1 + δ)</code>; additive stats apply as{" "}
          <code>value + δ</code>.
        </p>
        <table
          style={{
            borderCollapse: "collapse",
            fontSize: "0.9rem",
            fontVariantNumeric: "tabular-nums",
          }}
        >
          <thead>
            <tr style={{ borderBottom: "1px solid var(--border)" }}>
              <th style={{ textAlign: "left", padding: "0.35rem 0.5rem" }}>
                Stat
              </th>
              <th style={styles.cellRight}>δ</th>
              <th style={styles.cellCenter}>Mode</th>
              <th style={styles.cellCenter}>Include</th>
            </tr>
          </thead>
          <tbody>
            {defaults.map((row, i) => (
              <tr
                key={row.stat}
                style={{
                  background:
                    i % 2 === 1 ? "rgba(255,255,255,0.03)" : undefined,
                }}
              >
                <td style={{ padding: "0.35rem 0.5rem" }}>{row.stat}</td>
                <td style={styles.cellRight}>
                  <input
                    type="number"
                    step="0.001"
                    value={row.delta}
                    onChange={(e) => {
                      const next = [...defaults];
                      next[i] = { ...row, delta: Number(e.target.value) };
                      setDefaults(next);
                    }}
                    style={{ width: 90, textAlign: "right" }}
                  />
                </td>
                <td
                  style={{
                    textAlign: "center",
                    padding: "0.35rem 0.5rem",
                    color: "var(--text-muted)",
                    fontSize: "0.8rem",
                  }}
                >
                  {row.multiplicative ? "×(1+δ)" : "+δ"}
                </td>
                <td style={styles.cellCenter}>
                  <input
                    type="checkbox"
                    checked={row.enabled}
                    onChange={(e) => {
                      const next = [...defaults];
                      next[i] = { ...row, enabled: e.target.checked };
                      setDefaults(next);
                    }}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section style={styles.sectionTop}>
        <div style={{ display: "flex", gap: "0.75rem", alignItems: "center" }}>
          <button
            type="button"
            onClick={handleRun}
            disabled={!canRun || running}
            style={{
              padding: "0.6rem 1.25rem",
              background: "var(--accent)",
              color: "var(--bg)",
              border: "none",
              borderRadius: 4,
              cursor: canRun && !running ? "pointer" : "not-allowed",
              opacity: canRun && !running ? 1 : 0.5,
            }}
          >
            {running ? "Running…" : "Run sensitivity analysis"}
          </button>
          {running && jobId && (
            <button
              type="button"
              onClick={handleCancel}
              style={{
                padding: "0.55rem 1rem",
                background: "transparent",
                color: "var(--text)",
                border: "1px solid var(--border)",
                borderRadius: 4,
                cursor: "pointer",
              }}
            >
              Cancel
            </button>
          )}
        </div>

        {/* CPU-busy queue indicator: shown while sensitivityStart is waiting for a CPU
            permit (server returned 503 cpu_busy and is retrying with backoff). */}
        {cpuBusyInfo && (
          <div
            style={{
              marginTop: "0.75rem",
              padding: "0.5rem 0.75rem",
              background: "rgba(255,255,255,0.04)",
              border: "1px solid var(--border)",
              borderRadius: 4,
              fontSize: "0.85rem",
              color: "var(--text-muted)",
            }}
          >
            Server CPU is busy (another sim is running). Retrying in ≈
            {Math.max(1, Math.round(cpuBusyInfo.waitMs / 1000))}s (attempt{" "}
            {cpuBusyInfo.attempt})…
          </div>
        )}

        {/* Live progress while a job is running. Hidden once the job hits done/error. */}
        {running && jobId && (
          <div
            style={{
              marginTop: "0.75rem",
              padding: "0.6rem 0.85rem",
              border: "1px solid var(--border)",
              borderRadius: 6,
              background: "rgba(255,255,255,0.02)",
              fontSize: "0.88rem",
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                gap: "1rem",
                color: "var(--text-muted)",
              }}
            >
              <span>
                <strong>
                  {formatSensitivityPhaseLabel(jobPhase) || "Starting…"}
                </strong>
                {jobSimsDone != null &&
                  jobTotalSims != null &&
                  jobTotalSims > 0 && (
                    <>
                      {" · "}
                      {jobSimsDone.toLocaleString()} /{" "}
                      {jobTotalSims.toLocaleString()} sims
                    </>
                  )}
              </span>
              <span>
                {jobThroughput != null && (
                  <>~{jobThroughput.toFixed(0)} sims/s</>
                )}
                {jobEtaSeconds != null && <> · ETA ~{jobEtaSeconds}s</>}
              </span>
            </div>
            <div
              style={{
                marginTop: "0.4rem",
                height: 6,
                background: "rgba(255,255,255,0.05)",
                borderRadius: 3,
                overflow: "hidden",
              }}
            >
              <div
                style={{
                  width: `${Math.max(2, Math.min(100, jobProgress ?? 0))}%`,
                  height: "100%",
                  background: "var(--accent)",
                  transition: "width 200ms ease-out",
                }}
              />
            </div>
          </div>
        )}

        {error && (
          <div
            style={{
              marginTop: "0.75rem",
              padding: "0.5rem 0.75rem",
              background: "var(--error)",
              color: "white",
              borderRadius: 4,
            }}
            role="alert"
          >
            {error}
          </div>
        )}
      </section>

      {response && (
        <section style={styles.sectionTop}>
          <h3>Results</h3>
          <SensitivityResults
            rows={response.rows}
            baselineMean={response.baseline_mean}
            metric={response.metric}
            numSims={response.num_sims}
          />
        </section>
      )}

      {morrisResponse && (
        <section style={styles.sectionTop}>
          <h3>Results (Morris screening)</h3>
          <MorrisResults
            rows={morrisResponse.rows}
            metric={morrisResponse.metric}
            rTrajectories={morrisResponse.r_trajectories}
            numSimsPerPoint={morrisResponse.num_sims_per_point}
            totalSims={morrisResponse.total_sims}
            baseSeed={morrisResponse.base_seed}
          />
        </section>
      )}

      {sobolResponse && (
        <section style={styles.sectionTop}>
          <h3>Results (Sobol variance decomposition)</h3>
          <SobolResults
            rows={sobolResponse.rows}
            metric={sobolResponse.metric}
            nSamples={sobolResponse.n_samples}
            totalSims={sobolResponse.total_sims}
            outputVariance={sobolResponse.output_variance}
            baseSeed={sobolResponse.base_seed}
            pairs={sobolResponse.pairs}
          />
        </section>
      )}
    </div>
  );
}
