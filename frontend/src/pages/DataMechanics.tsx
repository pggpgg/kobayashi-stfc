import { useEffect, useState } from "react";
import {
  type DataVersionResponse,
  fetchDataVersion,
  fetchMechanicsCoverage,
  formatApiError,
  type MechanicsCoverageResponse,
  type MechanicsTierCounts,
} from "../lib/api";

function CoverageCard({
  title,
  detail,
  counts,
}: {
  title: string;
  detail: string;
  counts: MechanicsTierCounts;
}) {
  const total = counts.implemented + counts.partial + counts.ignored;
  const implementedPct = total > 0 ? (counts.implemented / total) * 100 : 0;
  const partialPct = total > 0 ? (counts.partial / total) * 100 : 0;

  return (
    <article
      style={{
        padding: "1rem",
        background: "var(--surface)",
        border: "1px solid var(--border)",
        borderRadius: 8,
        minWidth: 220,
      }}
    >
      <h3 style={{ margin: 0, fontSize: "1rem" }}>{title}</h3>
      <p
        style={{
          margin: "0.25rem 0 0.8rem",
          color: "var(--text-muted)",
          fontSize: "0.8rem",
        }}
      >
        {detail}
      </p>
      <div
        role="img"
        aria-label={`${title} coverage`}
        style={{
          display: "flex",
          height: 10,
          overflow: "hidden",
          borderRadius: 5,
          background: "var(--border)",
        }}
      >
        <span
          title={`${counts.implemented} implemented`}
          style={{ width: `${implementedPct}%`, background: "var(--success)" }}
        />
        <span
          title={`${counts.partial} partial`}
          style={{ width: `${partialPct}%`, background: "var(--warning)" }}
        />
      </div>
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: "0.45rem 0.9rem",
          marginTop: "0.75rem",
          fontSize: "0.82rem",
        }}
      >
        <span>
          <strong>{counts.implemented}</strong> implemented
        </span>
        <span>
          <strong>{counts.partial}</strong> partial
        </span>
        <span>
          <strong>{counts.ignored}</strong> ignored / out of scope
        </span>
      </div>
    </article>
  );
}

function areaLabel(area: string): string {
  const labels: Record<string, string> = {
    lcars: "Officer effects",
    ship_hull_abilities: "Ship hull abilities",
    hostile_ability_catalog: "Hostile abilities",
  };
  return labels[area] ?? area.split("_").join(" ");
}

export default function DataMechanics() {
  const [versions, setVersions] = useState<DataVersionResponse | null>(null);
  const [coverage, setCoverage] = useState<MechanicsCoverageResponse | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([fetchDataVersion(), fetchMechanicsCoverage()])
      .then(([nextVersions, nextCoverage]) => {
        if (cancelled) return;
        setVersions(nextVersions);
        setCoverage(nextCoverage);
      })
      .catch((e) => {
        if (!cancelled) setError(formatApiError(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (error) {
    return (
      <div>
        <h1>Data & Mechanics</h1>
        <p style={{ color: "var(--error)" }}>{error}</p>
      </div>
    );
  }

  if (!versions || !coverage) {
    return (
      <div>
        <h1>Data & Mechanics</h1>
        <p>Loading live coverage…</p>
      </div>
    );
  }

  return (
    <div>
      <h1 style={{ marginBottom: "0.35rem" }}>Data & Mechanics</h1>
      <p
        style={{
          margin: "0 0 1.25rem",
          color: "var(--text-muted)",
          maxWidth: 820,
        }}
      >
        Live coverage from the same catalogs and effect resolvers used by the
        simulator. Partial and ignored rows may be intentional non-combat scope;
        review the fidelity backlog before relying on an affected mechanic.
      </p>

      <section
        style={{
          marginBottom: "1.25rem",
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
            gap: "0.5rem 1.25rem",
            fontSize: "0.9rem",
          }}
        >
          <span>
            <strong>Officer catalog:</strong> {versions.officer_version ?? "—"}
          </span>
          <span>
            <strong>Hostile catalog:</strong> {versions.hostile_version ?? "—"}
          </span>
          <span>
            <strong>Ship catalog:</strong> {versions.ship_version ?? "—"}
          </span>
        </div>
      </section>

      <section aria-labelledby="coverage-heading">
        <h2 id="coverage-heading" style={{ fontSize: "1.05rem" }}>
          Live mechanics coverage
        </h2>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(230px, 1fr))",
            gap: "0.8rem",
          }}
        >
          <CoverageCard
            title="Officer effects"
            detail={`${coverage.lcars_officers_files} officer files scanned`}
            counts={coverage.lcars_effects}
          />
          <CoverageCard
            title="Ship hull abilities"
            detail={`${coverage.ships_with_abilities_scanned} ships with abilities scanned`}
            counts={coverage.ship_hull_abilities}
          />
          <CoverageCard
            title="Hostile abilities"
            detail={`${coverage.hostile_catalog_entry_count} catalog entries; ${coverage.hostile_upstream_ids_missing_from_catalog} missing upstream mappings`}
            counts={coverage.hostile_catalog_entries}
          />
        </div>
      </section>

      <section
        aria-labelledby="backlog-heading"
        style={{
          marginTop: "1.25rem",
          padding: "1rem",
          background: "var(--surface)",
          border: "1px solid var(--border)",
          borderRadius: 8,
        }}
      >
        <h2
          id="backlog-heading"
          style={{ margin: "0 0 0.25rem", fontSize: "1rem" }}
        >
          Fidelity backlog
        </h2>
        <p
          style={{
            margin: "0 0 0.75rem",
            color: "var(--text-muted)",
            fontSize: "0.85rem",
          }}
        >
          Ordered by unsupported effect count. This is coverage, not empirical
          accuracy; recorded-fight calibration is tracked separately.
        </p>
        {coverage.fidelity_backlog.length === 0 ? (
          <p style={{ margin: 0 }}>No coverage gaps reported.</p>
        ) : (
          <div style={{ overflowX: "auto" }}>
            <table
              style={{
                width: "100%",
                borderCollapse: "collapse",
                fontSize: "0.88rem",
              }}
            >
              <thead>
                <tr style={{ borderBottom: "1px solid var(--border)" }}>
                  <th style={{ textAlign: "left", padding: "0.5rem" }}>#</th>
                  <th style={{ textAlign: "left", padding: "0.5rem" }}>Area</th>
                  <th style={{ textAlign: "right", padding: "0.5rem" }}>
                    Implemented
                  </th>
                  <th style={{ textAlign: "right", padding: "0.5rem" }}>
                    Partial
                  </th>
                  <th style={{ textAlign: "right", padding: "0.5rem" }}>
                    Ignored
                  </th>
                  <th style={{ textAlign: "left", padding: "0.5rem" }}>
                    Detail
                  </th>
                </tr>
              </thead>
              <tbody>
                {coverage.fidelity_backlog.map((item) => (
                  <tr
                    key={`${item.area}-${item.key}`}
                    style={{ borderBottom: "1px solid var(--border)" }}
                  >
                    <td style={{ padding: "0.5rem" }}>{item.rank}</td>
                    <td style={{ padding: "0.5rem" }}>
                      {areaLabel(item.area)}
                    </td>
                    <td style={{ padding: "0.5rem", textAlign: "right" }}>
                      {item.implemented}
                    </td>
                    <td style={{ padding: "0.5rem", textAlign: "right" }}>
                      {item.partial}
                    </td>
                    <td style={{ padding: "0.5rem", textAlign: "right" }}>
                      {item.ignored}
                    </td>
                    <td
                      style={{ padding: "0.5rem", color: "var(--text-muted)" }}
                    >
                      {item.summary}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}
