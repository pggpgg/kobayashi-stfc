import { useEffect, useMemo, useState } from "react";
import type { DataVersionResponse, MechanicsCoverageReport } from "../lib/api";
import {
  fetchDataVersion,
  fetchMechanicsCoverage,
  formatApiError,
} from "../lib/api";

export default function DataMechanics() {
  const [data, setData] = useState<DataVersionResponse | null>(null);
  const [coverage, setCoverage] = useState<MechanicsCoverageReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let c = false;
    fetchDataVersion()
      .then((v) => {
        if (!c) setData(v);
      })
      .catch((e) => {
        if (!c) setError(formatApiError(e));
      });
    fetchMechanicsCoverage()
      .then((v) => {
        if (!c) setCoverage(v);
      })
      .catch((e) => {
        if (!c && !error) setError(formatApiError(e));
      });
    return () => {
      c = true;
    };
  }, [error]);

  const backlogTop = useMemo(() => {
    const list = coverage?.fidelity_backlog ?? [];
    return list.slice(0, 12);
  }, [coverage]);

  if (error) {
    return (
      <div>
        <h1>Data & Mechanics</h1>
        <p style={{ color: "var(--error)" }}>{error}</p>
      </div>
    );
  }

  if (!data || !coverage) {
    return (
      <div>
        <h1>Data & Mechanics</h1>
        <p>Loading…</p>
      </div>
    );
  }

  return (
    <div>
      <h1 style={{ marginBottom: "1rem" }}>Data & Mechanics</h1>

      <section
        style={{
          marginBottom: "1.5rem",
          padding: "1rem",
          background: "var(--surface)",
          border: "1px solid var(--border)",
          borderRadius: 8,
        }}
      >
        <h2 style={{ margin: "0 0 0.75rem", fontSize: "1rem" }}>
          Data version
        </h2>
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "1rem",
            fontSize: "0.9rem",
          }}
        >
          <span>
            <strong>Officer catalog:</strong> {data.officer_version ?? "—"}
          </span>
          <span>
            <strong>Hostile catalog:</strong> {data.hostile_version ?? "—"}
          </span>
          <span>
            <strong>Ship catalog:</strong> {data.ship_version ?? "—"}
          </span>
        </div>
      </section>

      <section
        style={{
          padding: "1rem",
          background: "var(--surface)",
          border: "1px solid var(--border)",
          borderRadius: 8,
          marginBottom: "1.5rem",
        }}
      >
        <h2 style={{ margin: "0 0 0.75rem", fontSize: "1rem" }}>
          Mechanics coverage
        </h2>
        <table
          style={{
            width: "100%",
            borderCollapse: "collapse",
            fontSize: "0.9rem",
          }}
        >
          <thead>
            <tr style={{ borderBottom: "1px solid var(--border)" }}>
              <th style={{ textAlign: "left", padding: "0.5rem" }}>Mechanic</th>
              <th style={{ textAlign: "left", padding: "0.5rem" }}>Status</th>
            </tr>
          </thead>
          <tbody>
            {data.mechanics.map((m) => (
              <tr
                key={m.name}
                style={{ borderBottom: "1px solid var(--border)" }}
              >
                <td style={{ padding: "0.5rem" }}>{m.name}</td>
                <td style={{ padding: "0.5rem" }}>
                  <span
                    style={{
                      padding: "0.2rem 0.5rem",
                      borderRadius: 4,
                      background:
                        m.status === "implemented"
                          ? "var(--success)"
                          : m.status === "partial"
                            ? "var(--warning)"
                            : "var(--text-muted)",
                      color:
                        m.status === "planned"
                          ? "var(--text-muted)"
                          : "var(--bg)",
                    }}
                  >
                    {m.status}
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <p
          style={{
            marginTop: "0.75rem",
            fontSize: "0.85rem",
            color: "var(--text-muted)",
          }}
        >
          Burning applies to the hostile in ship-vs-hostile sims (fixed 1% of
          target max hull per active round). Hostile abilities that burn your
          ship are not modeled yet.
        </p>
      </section>

      <section
        style={{
          padding: "1rem",
          background: "var(--surface)",
          border: "1px solid var(--border)",
          borderRadius: 8,
        }}
      >
        <h2 style={{ margin: "0 0 0.75rem", fontSize: "1rem" }}>
          Fidelity backlog (top gaps)
        </h2>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(320px, 1fr))",
            gap: "0.75rem",
          }}
        >
          {backlogTop.map((b) => (
            <div
              key={`${b.area}:${b.key}`}
              style={{
                padding: "0.75rem",
                border: "1px solid var(--border)",
                borderRadius: 8,
                background: "var(--bg)",
              }}
            >
              <div style={{ display: "flex", justifyContent: "space-between" }}>
                <strong>
                  #{b.rank} {b.area} / {b.key}
                </strong>
                <span style={{ color: "var(--text-muted)", fontSize: "0.85rem" }}>
                  {b.ignored} ignored · {b.partial} partial · {b.implemented} ok
                </span>
              </div>
              <div
                style={{
                  marginTop: "0.4rem",
                  fontSize: "0.9rem",
                  color: "var(--text-muted)",
                  lineHeight: 1.35,
                }}
              >
                {b.summary}
              </div>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
