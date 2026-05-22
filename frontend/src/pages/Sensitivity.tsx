import { useEffect, useMemo, useState } from "react";
import HostilePicker from "../components/HostilePicker";
import MorrisResults from "../components/MorrisResults";
import SensitivityResults from "../components/SensitivityResults";
import SobolResults from "../components/SobolResults";
import { useProfile } from "../contexts/ProfileContext";
import {
  ApiError,
  fetchHostiles,
  fetchShips,
  type HostileListItem,
  type ShipListItem,
} from "../lib/api";
import {
  fetchMorrisDefaults,
  fetchSensitivityDefaults,
  fetchSobolDefaults,
  type MorrisResponse,
  type OutcomeMetric,
  runMorris,
  runSensitivity,
  runSobol,
  type SensitivityDefaultRow,
  type SensitivityResponse,
  type SobolResponse,
} from "../lib/sensitivityApi";

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
  const [sobolResponse, setSobolResponse] = useState<SobolResponse | null>(
    null,
  );

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

  async function handleRun() {
    if (!canRun) return;
    setRunning(true);
    setError(null);
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
      if (method === "oat") {
        const result = await runSensitivity(
          { ...sharedScenario, num_sims: numSims },
          activeProfileId,
        );
        setResponse(result);
        setMorrisResponse(null);
        setSobolResponse(null);
      } else if (method === "morris") {
        const result = await runMorris(
          {
            ...sharedScenario,
            num_sims: morrisNumSims,
            r_trajectories: rTrajectories,
          },
          activeProfileId,
        );
        setMorrisResponse(result);
        setResponse(null);
        setSobolResponse(null);
      } else {
        const result = await runSobol(
          { ...sharedScenario, n_samples: sobolNSamples },
          activeProfileId,
        );
        setSobolResponse(result);
        setResponse(null);
        setMorrisResponse(null);
      }
    } catch (e: unknown) {
      if (e instanceof ApiError) {
        setError(`${e.code}: ${e.message}`);
      } else if (e instanceof Error) {
        setError(e.message);
      } else {
        setError("Unknown error");
      }
    } finally {
      setRunning(false);
    }
  }

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
        <h3 style={{ marginBottom: "0.5rem" }}>Scenario</h3>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
            gap: "0.75rem",
          }}
        >
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
              Ship
            </span>
            <select value={shipId} onChange={(e) => setShipId(e.target.value)}>
              <option value="">— pick a ship —</option>
              {ships.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.ship_name}
                </option>
              ))}
            </select>
          </label>
          <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
              Hostile
            </span>
            <HostilePicker
              hostiles={hostiles}
              value={scenarioId}
              onChange={setScenarioId}
            />
          </div>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
              Ship tier
            </span>
            <input
              type="number"
              min={1}
              max={15}
              value={shipTier}
              onChange={(e) => setShipTier(Number(e.target.value))}
            />
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
              Ship level
            </span>
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
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
              Captain id
            </span>
            <input
              type="text"
              value={captain}
              onChange={(e) => setCaptain(e.target.value)}
              placeholder="e.g. ent-e-picard-556227"
            />
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
              Bridge officer ids (comma-separated)
            </span>
            <input
              type="text"
              value={bridgeText}
              onChange={(e) => setBridgeText(e.target.value)}
              placeholder="ent-e-data-871245, five-of-eleven-d9aa11"
            />
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
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

      <section style={{ marginTop: "1.5rem" }}>
        <h3 style={{ marginBottom: "0.5rem" }}>Run parameters</h3>
        <div style={{ marginBottom: "0.75rem" }}>
          <label
            style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}
            htmlFor="sensitivity-method"
          >
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
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
              Metric
            </span>
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
            <span style={{ fontSize: "0.75rem", color: "var(--text-muted)" }}>
              {METRICS.find((m) => m.value === metric)?.hint}
            </span>
          </label>
          {method === "oat" && (
            <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
                Paired sims per stat
              </span>
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
              <label
                style={{ display: "flex", flexDirection: "column", gap: 4 }}
              >
                <span
                  style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}
                >
                  r trajectories
                </span>
                <input
                  type="number"
                  min={2}
                  max={200}
                  value={rTrajectories}
                  onChange={(e) => setRTrajectories(Number(e.target.value))}
                />
                <span
                  style={{ fontSize: "0.75rem", color: "var(--text-muted)" }}
                >
                  10 is a conservative default; 20–50 tightens μ* / σ at
                  proportional cost.
                </span>
              </label>
              <label
                style={{ display: "flex", flexDirection: "column", gap: 4 }}
              >
                <span
                  style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}
                >
                  Sims per trajectory point
                </span>
                <input
                  type="number"
                  min={2}
                  max={10000}
                  value={morrisNumSims}
                  onChange={(e) => setMorrisNumSims(Number(e.target.value))}
                />
                <span
                  style={{ fontSize: "0.75rem", color: "var(--text-muted)" }}
                >
                  Total sims ≈ r × (k+1) × this.
                </span>
              </label>
            </>
          )}
          {method === "sobol" && (
            <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
                N samples per Saltelli matrix
              </span>
              <input
                type="number"
                min={8}
                max={8192}
                value={sobolNSamples}
                onChange={(e) => setSobolNSamples(Number(e.target.value))}
              />
              <span style={{ fontSize: "0.75rem", color: "var(--text-muted)" }}>
                Total sims = N × (k + 2). N=512 is a reasonable default; raise
                to 2048 for tighter CIs.
              </span>
            </label>
          )}
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
              Base seed
            </span>
            <input
              type="number"
              min={0}
              value={seed}
              onChange={(e) => setSeed(Number(e.target.value))}
            />
          </label>
        </div>
      </section>

      <section style={{ marginTop: "1.5rem" }}>
        <h3 style={{ marginBottom: "0.5rem" }}>Per-stat overrides</h3>
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
              <th style={{ textAlign: "right", padding: "0.35rem 0.5rem" }}>
                δ
              </th>
              <th style={{ textAlign: "center", padding: "0.35rem 0.5rem" }}>
                Mode
              </th>
              <th style={{ textAlign: "center", padding: "0.35rem 0.5rem" }}>
                Include
              </th>
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
                <td style={{ textAlign: "right", padding: "0.35rem 0.5rem" }}>
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
                <td style={{ textAlign: "center", padding: "0.35rem 0.5rem" }}>
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

      <section style={{ marginTop: "1.5rem" }}>
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
        <section style={{ marginTop: "1.5rem" }}>
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
        <section style={{ marginTop: "1.5rem" }}>
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
        <section style={{ marginTop: "1.5rem" }}>
          <h3>Results (Sobol variance decomposition)</h3>
          <SobolResults
            rows={sobolResponse.rows}
            metric={sobolResponse.metric}
            nSamples={sobolResponse.n_samples}
            totalSims={sobolResponse.total_sims}
            outputVariance={sobolResponse.output_variance}
            baseSeed={sobolResponse.base_seed}
          />
        </section>
      )}
    </div>
  );
}
