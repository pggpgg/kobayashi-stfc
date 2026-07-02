import type { CSSProperties, ReactNode } from "react";

/**
 * Learn — how Kobayashi searches crew space.
 *
 * Static teaching content distilled from docs/CREW_OPTIMIZATION_METHODS.md,
 * docs/OPTIMIZATION_SPECIAL_HEURISTICS.md, and
 * docs/OPTIMIZER_AMBITIOUS_ROADMAP.md. Update those docs first; this page
 * summarizes them for players.
 */

const DOCS_BASE = "https://github.com/pggpgg/kobayashi-stfc/blob/main/docs";

type MethodStatus = "implemented" | "roadmap";

interface MethodEntry {
  id: string;
  title: string;
  status: MethodStatus;
  /** Optional request/provenance label rendered as a code chip. */
  apiLabel?: string;
  /** What the method is, in plain language. */
  what: string;
  /** How Kobayashi uses it today, or what the roadmap plans. */
  detail: string;
  /** One-line guidance on when it shines. */
  when: string;
  docHref?: string;
  /** Optional extra visual rendered under the text. */
  extra?: ReactNode;
}

const styles: Record<string, CSSProperties> = {
  page: { maxWidth: 980 },
  intro: {
    margin: "0 0 1.25rem",
    color: "var(--text-muted)",
    maxWidth: 820,
  },
  section: { marginBottom: "1.5rem" },
  sectionHint: {
    margin: "0 0 0.9rem",
    color: "var(--text-muted)",
    fontSize: "0.85rem",
    maxWidth: 820,
  },
  cardGrid: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fit, minmax(290px, 1fr))",
    gap: "0.8rem",
  },
  card: {
    padding: "1rem",
    background: "var(--surface)",
    border: "1px solid var(--border)",
    borderRadius: 8,
    display: "flex",
    flexDirection: "column",
    gap: "0.5rem",
  },
  cardHeader: {
    display: "flex",
    alignItems: "baseline",
    justifyContent: "space-between",
    gap: "0.5rem",
    flexWrap: "wrap",
  },
  cardTitle: { margin: 0, fontSize: "0.98rem" },
  badgeImplemented: {
    fontSize: "0.68rem",
    textTransform: "uppercase",
    letterSpacing: "0.04em",
    padding: "0.15rem 0.5rem",
    borderRadius: 999,
    background: "color-mix(in srgb, var(--success) 18%, transparent)",
    color: "var(--success)",
    border: "1px solid var(--success)",
    whiteSpace: "nowrap",
  },
  badgeRoadmap: {
    fontSize: "0.68rem",
    textTransform: "uppercase",
    letterSpacing: "0.04em",
    padding: "0.15rem 0.5rem",
    borderRadius: 999,
    background: "color-mix(in srgb, var(--warning) 15%, transparent)",
    color: "var(--warning)",
    border: "1px solid var(--warning)",
    whiteSpace: "nowrap",
  },
  apiChip: {
    fontFamily: "monospace",
    fontSize: "0.72rem",
    color: "var(--accent)",
    background: "var(--bg)",
    border: "1px solid var(--border)",
    borderRadius: 4,
    padding: "0.1rem 0.4rem",
    alignSelf: "flex-start",
  },
  body: { margin: 0, fontSize: "0.85rem", lineHeight: 1.5 },
  detail: {
    margin: 0,
    fontSize: "0.82rem",
    lineHeight: 1.5,
    color: "var(--text-muted)",
  },
  when: {
    margin: 0,
    fontSize: "0.78rem",
    color: "var(--text-muted)",
    fontStyle: "italic",
  },
  docLink: { fontSize: "0.78rem", color: "var(--accent)" },
  diagramBox: {
    padding: "1rem",
    background: "var(--surface)",
    border: "1px solid var(--border)",
    borderRadius: 8,
    overflowX: "auto",
  },
};

/** The candidate funnel every optimize run flows through. */
function PipelineFunnelDiagram() {
  const stages: Array<{ label: string; note: string; width: number }> = [
    { label: "Officer catalog", note: "every known officer", width: 620 },
    {
      label: "Role pools",
      note: "roster owned · ban list · scenario eligibility",
      width: 520,
    },
    {
      label: "Candidate crews",
      note: "legal captain/bridge/below-decks tuples + warm start & seeds",
      width: 430,
    },
    {
      label: "Constraints",
      note: "must-include / exclude / groups",
      width: 360,
    },
    {
      label: "Analytical prefilter",
      note: "closed-form proxy score keeps the plausible slice",
      width: 280,
    },
    {
      label: "Scout sims",
      note: "few hundred cheap Monte Carlo trials per crew",
      width: 200,
    },
    {
      label: "Confirm top K",
      note: "full simulation depth + confidence intervals",
      width: 120,
    },
  ];
  const rowHeight = 46;
  const svgWidth = 660;
  const height = stages.length * rowHeight + 10;
  return (
    <svg
      width={svgWidth}
      height={height}
      viewBox={`0 0 ${svgWidth} ${height}`}
      role="img"
      aria-label="Candidate funnel: officer catalog narrows through role pools, candidate generation, constraints, analytical prefilter, and scout simulation down to full confirmation of the top K crews"
      style={{ maxWidth: "100%", height: "auto" }}
    >
      <title>Kobayashi optimize pipeline funnel</title>
      {stages.map((stage, i) => {
        const y = i * rowHeight + 5;
        const x = (svgWidth - stage.width) / 2;
        const isEnds = i === 0 || i === stages.length - 1;
        return (
          <g key={stage.label}>
            <rect
              x={x}
              y={y}
              width={stage.width}
              height={rowHeight - 12}
              rx={6}
              fill={
                isEnds
                  ? "color-mix(in srgb, var(--accent) 22%, transparent)"
                  : "color-mix(in srgb, var(--accent) 10%, transparent)"
              }
              stroke="var(--border)"
            />
            <text
              x={svgWidth / 2}
              y={y + 15}
              textAnchor="middle"
              fill="var(--text)"
              fontSize={12.5}
              fontWeight={600}
            >
              {stage.label}
            </text>
            <text
              x={svgWidth / 2}
              y={y + 29}
              textAnchor="middle"
              fill="var(--text-muted)"
              fontSize={10.5}
            >
              {stage.note}
            </text>
            {i < stages.length - 1 && (
              <text
                x={svgWidth / 2}
                y={y + rowHeight - 3}
                textAnchor="middle"
                fill="var(--text-muted)"
                fontSize={10}
              >
                ▼
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}

/** Mini bar pair showing where tiered spends its simulation budget. */
function TieredBudgetMiniDiagram() {
  return (
    <svg
      width={260}
      height={64}
      viewBox="0 0 260 64"
      role="img"
      aria-label="Tiered budget: many crews get a shallow scout pass; only the top K get full-depth confirmation"
      style={{ maxWidth: "100%", height: "auto" }}
    >
      <title>Tiered scout versus confirm budget</title>
      <rect
        x={0}
        y={6}
        width={250}
        height={16}
        rx={4}
        fill="color-mix(in srgb, var(--accent) 12%, transparent)"
        stroke="var(--border)"
      />
      <text x={6} y={18} fontSize={10.5} fill="var(--text)">
        scout: all candidates × few hundred sims
      </text>
      <rect
        x={0}
        y={36}
        width={70}
        height={16}
        rx={4}
        fill="color-mix(in srgb, var(--success) 22%, transparent)"
        stroke="var(--success)"
      />
      <text x={78} y={48} fontSize={10.5} fill="var(--text-muted)">
        confirm: top K × full sims
      </text>
    </svg>
  );
}

const IMPLEMENTED_METHODS: MethodEntry[] = [
  {
    id: "exact-filters",
    title: "Exact pruning & eligibility filters",
    status: "implemented",
    what: "Before anything is simulated, provably irrelevant officers are removed: the curated ban list, per-scenario eligibility (an ability that cannot fire against this enemy type disqualifies the seat), and your synced roster narrow the captain, bridge, and below-decks pools.",
    detail:
      "These are exact filters, not guesses — they only drop officers whose abilities verifiably cannot contribute. Combined with search constraints (must-include, exclude, groups) they cut the full-catalog space by 46×–5,400× depending on the scenario.",
    when: "Always on; this is why searches finish in seconds instead of days.",
    docHref: `${DOCS_BASE}/PVE_CREW_SEARCH_SPACE_REDUCTION.md`,
  },
  {
    id: "analytical-prefilter",
    title: "Analytical prefilter (proxy scoring)",
    status: "implemented",
    what: "A closed-form estimate of expected hull damage ranks every candidate without running combat. Only the most plausible slice moves on to Monte Carlo simulation.",
    detail:
      "The keep count auto-scales with workload, and learned priors (officer pair co-occurrence from your warm-start and history) can nudge the ranking. Proxy scores never reach the final results — they only decide who gets simulated.",
    when: "Kicks in automatically on large candidate sets; skipped for chain grind.",
    docHref: `${DOCS_BASE}/OPTIMIZATION_SPECIAL_HEURISTICS.md`,
  },
  {
    id: "exhaustive",
    title: "Exhaustive Monte Carlo",
    status: "implemented",
    apiLabel: 'strategy: "exhaustive"',
    what: "Every remaining candidate gets your full simulation count. Exact and easy to trust, but cost grows combinatorially with roster size and below-decks slots.",
    detail:
      "An optional two-phase mode (scout sims + top-keep) borrows the tiered idea: shallow trials on everyone, full depth only on the leaders.",
    when: "Best when the post-filter space is small or you want a reference answer.",
    docHref: `${DOCS_BASE}/CREW_OPTIMIZATION_METHODS.md`,
  },
  {
    id: "tiered",
    title: "Tiered scout → confirm",
    status: "implemented",
    apiLabel: 'strategy: "tiered"',
    what: "Two-pass search: a cheap scouting pass (a few hundred trials per crew) ranks all candidates, then only the top K receive your full simulation depth with confidence intervals.",
    detail:
      "The scout phase is adaptive — a coarse pass first, then refinement where rankings are uncertain (Wilson interval widths decide who needs more trials). An optional priority-queue scheduler promotes promising crews sooner and abandons hopeless ones early. This is the SPA default.",
    when: "The default for medium and large searches; near-exhaustive quality at a fraction of the cost.",
    docHref: `${DOCS_BASE}/OPTIMIZATION_SPECIAL_HEURISTICS.md`,
    extra: <TieredBudgetMiniDiagram />,
  },
  {
    id: "genetic",
    title: "Genetic algorithm",
    status: "implemented",
    apiLabel: 'strategy: "genetic"',
    what: "Evolves populations of crews: strong crews reproduce, officers mutate slot by slot, and weak lineages die out. Explores enormous spaces without enumerating them.",
    detail:
      "Can be seeded with heuristics crews (seeded GA) so evolution starts from known-good lineups instead of random noise.",
    when: "Reach for it when the legal space is far too large to enumerate, even after filters.",
    docHref: `${DOCS_BASE}/CREW_OPTIMIZATION_METHODS.md`,
  },
  {
    id: "random-stratified",
    title: "Stratified random baseline",
    status: "implemented",
    apiLabel: 'strategy: "random_stratified"',
    what: "Legal crews sampled at random — stratified across captain faction/rarity cells and below-decks group families so rare corners of your roster get sampled as often as crowded ones — then scout → confirm.",
    detail:
      "This is the control group: if a clever method cannot beat well-stratified random sampling, it is not earning its complexity. A tiered option (random exploration %) also swaps a slice of the scout set for these crews so surprising lineups the analytical proxy would discard still get scouted. Result rows are labeled random_stratified.",
    when: "Use as a benchmark, or enable the tiered slice to hedge against proxy blind spots.",
    docHref: `${DOCS_BASE}/OPTIMIZER_AMBITIOUS_ROADMAP.md`,
  },
  {
    id: "linear-eval",
    title: "Linear eval",
    status: "implemented",
    apiLabel: 'strategy: "linear_eval"',
    what: "Ranks crews purely by closed-form expected hull damage — no Monte Carlo at all. Deterministic and nearly instant.",
    detail:
      "Win rates are not simulated, so multi-round proc variance and survival are invisible to it. Rows carry expected_hull_damage instead.",
    when: "Quick approximate exploration when you want an instant, deterministic ordering.",
    docHref: `${DOCS_BASE}/CREW_OPTIMIZATION_METHODS.md`,
  },
  {
    id: "heuristics-seeds",
    title: "Heuristics seeds & warm start",
    status: "implemented",
    what: "Community-known crews (data/heuristics/*.txt) are simulated before the broader search, and your previous winners persist as warm-start crews that re-enter every later run.",
    detail:
      "Fast discovery merges seed crews into the main optimize path instead of running them separately; a curated proven-crew seed guarantees at least one strong, legal lineup always reaches the candidate list. Officer names resolve through aliases and fuzzy matching.",
    when: "Steers the search toward lineups the community or your own history already trusts.",
    docHref: `${DOCS_BASE}/OPTIMIZATION_SPECIAL_HEURISTICS.md`,
  },
  {
    id: "learned-signals",
    title: "Learned signals & optimize history",
    status: "implemented",
    what: "Kobayashi remembers. Per-profile officer performance scores learned from past results bias below-decks sampling; confirmed crews are cached so repeat searches reuse their statistics instead of re-simulating.",
    detail:
      "The analytical prefilter can also use a learned pair-co-occurrence prior from your warm-start and history crews. All learning stays advisory — the full simulator always has the final word.",
    when: "Automatic with a profile; repeat searches on the same matchup get faster and smarter.",
    docHref: `${DOCS_BASE}/OPTIMIZATION_SPECIAL_HEURISTICS.md`,
  },
  {
    id: "auto-routing",
    title: "Strategy auto-routing",
    status: "implemented",
    what: "Omit the strategy entirely and the server counts effective candidates (after warm start and constraints) and picks tiered for large searches or exhaustive for small ones.",
    detail:
      "The response reports effective_strategy and strategy_auto so you always know what actually ran.",
    when: "The sensible default when you don't want to think about strategy at all.",
    docHref: `${DOCS_BASE}/OPTIMIZATION_SPECIAL_HEURISTICS.md`,
  },
];

const ROADMAP_METHODS: MethodEntry[] = [
  {
    id: "local-refine",
    title: "Local refinement & large-neighborhood repair",
    status: "roadmap",
    what: "After tiered or genetic finalists are known, search their neighborhoods: one-slot bridge and below-decks swaps, captain swaps, and destroy-repair moves that rebuild 2–3 seats from compatible pools.",
    detail:
      "Only improvements and diverse near-ties get confirmed. The payoff is explainable: “this recommendation improved the genetic winner by replacing X with Y.”",
    when: "Next planned quality jump — squeezes extra value out of every strong crew.",
    docHref: `${DOCS_BASE}/OPTIMIZER_AMBITIOUS_ROADMAP.md`,
  },
  {
    id: "pareto",
    title: "Pareto frontier recommendations",
    status: "roadmap",
    what: "Instead of one scalar score, compute the frontier over win rate, speed, hull remaining, variance, chain efficiency, and roster accessibility — then expose preset views: safest, fastest farming, lowest variance, best substitute.",
    detail:
      "The scalar score stays as the default sort; rows gain pareto_tags and a recommendation_reason.",
    when: "For choosing between speed, safety, and accessibility instead of trusting one number.",
    docHref: `${DOCS_BASE}/OPTIMIZER_AMBITIOUS_ROADMAP.md`,
  },
  {
    id: "beam",
    title: "Beam search with diversity lanes",
    status: "roadmap",
    what: "Builds crews seat by seat, keeping only the most promising partial crews at each step — with separate beams for damage, survivability, and round-1 kill so one meta archetype cannot consume the whole search.",
    detail:
      "Planned as an explicit discovery lane alongside tiered and genetic.",
    when: "Fast good-enough answers over billions of legal tuples.",
    docHref: `${DOCS_BASE}/OPTIMIZER_AMBITIOUS_ROADMAP.md`,
  },
  {
    id: "quality-diversity",
    title: "Quality-diversity archive (MAP-Elites)",
    status: "roadmap",
    what: "Keeps the best crew per behavior cell (archetype × faction × trigger package) rather than a single global winner, illuminating the whole space of viable crew families.",
    detail:
      "Feeds substitute recommendations: when you lack a key officer, the archive already knows the best crew in the nearest cell.",
    when: "Diverse recommendations and “what else works?” exploration.",
    docHref: `${DOCS_BASE}/OPTIMIZER_AMBITIOUS_ROADMAP.md`,
  },
  {
    id: "cross-entropy",
    title: "Cross-entropy sampler",
    status: "roadmap",
    what: "Learns a sampling distribution over officers and pairs from each round of results, then samples the next round from that sharpened distribution — random search that teaches itself where to look.",
    detail:
      "An estimation-of-distribution lane in the planned search portfolio.",
    when: "Large spaces where strong officer combinations repeat across winners.",
    docHref: `${DOCS_BASE}/OPTIMIZER_AMBITIOUS_ROADMAP.md`,
  },
  {
    id: "racing",
    title: "Hyperband-style racing",
    status: "roadmap",
    what: "Generalizes the tiered scout into successive-halving brackets: simulate everyone shallowly, repeatedly halve the field while deepening trials, so budget concentrates on statistical survivors.",
    detail:
      "Confidence bounds keep early lucky streaks from over-promoting weak crews.",
    when: "Stochastic objectives where equal budget per crew wastes most of it.",
    docHref: `${DOCS_BASE}/OPTIMIZER_AMBITIOUS_ROADMAP.md`,
  },
  {
    id: "surrogate",
    title: "Surrogate ranker & active learning",
    status: "roadmap",
    what: "Train a model on accumulated simulation observations to score millions of candidates instantly; fully simulate only the top slice, and keep retraining as new observations land.",
    detail:
      "Gated behind the observation log (already recording when enabled). A hard rule from the roadmap: no model-only final recommendations — the simulator always confirms.",
    when: "Once enough observation data exists; the most sample-efficient discovery lane.",
    docHref: `${DOCS_BASE}/OPTIMIZER_AMBITIOUS_ROADMAP.md`,
  },
  {
    id: "multi-scenario",
    title: "Robust multi-scenario optimization",
    status: "roadmap",
    what: "Optimize one crew against a set of hostiles (a farming route, an event pool) instead of a single target — plus substitute planning and campaign/chain policies.",
    detail:
      "Fleet-aware: which three crews cover tonight's targets best, given each officer can only sit in one ship.",
    when: "When the real question is a route or a fleet, not a single fight.",
    docHref: `${DOCS_BASE}/OPTIMIZER_AMBITIOUS_ROADMAP.md`,
  },
  {
    id: "meta-optimizer",
    title: "Meta-optimizer",
    status: "roadmap",
    what: "A scheduler over the whole portfolio: give it a compute budget and it allocates across lanes (tiered, genetic, beam, random, surrogate) based on which method has been earning its keep on similar searches.",
    detail:
      "The endgame of the ambitious roadmap — method choice itself becomes learned.",
    when: "The long-term destination once multiple lanes exist and telemetry can compare them.",
    docHref: `${DOCS_BASE}/OPTIMIZER_AMBITIOUS_ROADMAP.md`,
  },
];

function MethodCard({ entry }: { entry: MethodEntry }) {
  return (
    <article style={styles.card} aria-labelledby={`learn-${entry.id}`}>
      <div style={styles.cardHeader}>
        <h3 id={`learn-${entry.id}`} style={styles.cardTitle}>
          {entry.title}
        </h3>
        <span
          style={
            entry.status === "implemented"
              ? styles.badgeImplemented
              : styles.badgeRoadmap
          }
        >
          {entry.status === "implemented" ? "In Kobayashi" : "Roadmap"}
        </span>
      </div>
      {entry.apiLabel && <code style={styles.apiChip}>{entry.apiLabel}</code>}
      <p style={styles.body}>{entry.what}</p>
      <p style={styles.detail}>{entry.detail}</p>
      <p style={styles.when}>{entry.when}</p>
      {entry.extra}
      {entry.docHref && (
        <a
          href={entry.docHref}
          target="_blank"
          rel="noreferrer"
          style={styles.docLink}
        >
          Read more in the docs ↗
        </a>
      )}
    </article>
  );
}

export default function Learn() {
  return (
    <div style={styles.page}>
      <h1 style={{ marginBottom: "0.35rem" }}>Learn</h1>
      <p style={styles.intro}>
        How Kobayashi finds crews. The core principle: never simulate every crew
        equally. Exact filters remove officers that provably cannot help, cheap
        scoring ranks what remains, and expensive combat simulation is reserved
        for the candidates where it actually buys information. Every result row
        records which method produced it (<code>method_provenance</code>), so
        you can always tell how a recommendation was found.
      </p>

      <section style={styles.section} aria-labelledby="learn-pipeline-heading">
        <h2 id="learn-pipeline-heading" style={{ fontSize: "1.05rem" }}>
          How a search runs
        </h2>
        <p style={styles.sectionHint}>
          Every optimize request flows through the same funnel. Discovery
          (finding strong crews fast) and confirmation (spending deep simulation
          on the best) are deliberately separate phases.
        </p>
        <div style={styles.diagramBox}>
          <PipelineFunnelDiagram />
        </div>
      </section>

      <section style={styles.section} aria-labelledby="learn-today-heading">
        <h2 id="learn-today-heading" style={{ fontSize: "1.05rem" }}>
          Methods in Kobayashi today
        </h2>
        <p style={styles.sectionHint}>
          Pick a strategy explicitly in the Strategy panel, or omit it and let
          auto-routing choose. Everything below ships in the current optimizer.
        </p>
        <div style={styles.cardGrid}>
          {IMPLEMENTED_METHODS.map((entry) => (
            <MethodCard key={entry.id} entry={entry} />
          ))}
        </div>
      </section>

      <section style={styles.section} aria-labelledby="learn-roadmap-heading">
        <h2 id="learn-roadmap-heading" style={{ fontSize: "1.05rem" }}>
          On the roadmap
        </h2>
        <p style={styles.sectionHint}>
          The ambitious optimizer roadmap grows search into a portfolio of
          lanes, each validated against the stratified random baseline before it
          earns production budget.
        </p>
        <div style={styles.cardGrid}>
          {ROADMAP_METHODS.map((entry) => (
            <MethodCard key={entry.id} entry={entry} />
          ))}
        </div>
      </section>

      <section
        style={{
          ...styles.diagramBox,
          marginBottom: "1.5rem",
        }}
        aria-labelledby="learn-rules-heading"
      >
        <h2
          id="learn-rules-heading"
          style={{ margin: "0 0 0.5rem", fontSize: "1rem" }}
        >
          Quality rules every method follows
        </h2>
        <ul
          style={{
            margin: 0,
            paddingLeft: "1.2rem",
            fontSize: "0.85rem",
            lineHeight: 1.6,
            color: "var(--text-muted)",
          }}
        >
          <li>
            Exact filters stay separate from soft heuristics — hard exclusions
            must be provable, not vibes.
          </li>
          <li>
            An exploration budget is preserved so unusual crews can still
            surface (see the stratified random slice).
          </li>
          <li>
            Every recommendation reports its simulation depth and confidence
            intervals, and records which method discovered it.
          </li>
          <li>
            Heuristic and model-ranked winners are always confirmed by the full
            combat simulator before being presented as best.
          </li>
          <li>
            Fixed seeds make every search reproducible: same seed, same
            recommendations.
          </li>
        </ul>
      </section>
    </div>
  );
}
