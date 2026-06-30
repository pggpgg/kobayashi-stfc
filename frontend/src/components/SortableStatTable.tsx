import type { CSSProperties, ReactNode } from "react";

export type ColumnAlign = "left" | "right" | "center";
export type ColumnVariant = "plain" | "headline" | "ci";

export interface StatTableColumn<Row> {
  key: string;
  header: ReactNode;
  /** <th title=> tooltip. */
  headerTitle?: string;
  align?: ColumnAlign;
  /** "headline" = bold (the row's main number); "ci" = muted + smaller (a confidence interval). */
  variant?: ColumnVariant;
  /** Caller owns all numeric formatting (digit counts, brackets, glyphs). */
  render: (row: Row) => ReactNode;
}

export interface SortKeyOption<SortKey extends string> {
  key: SortKey;
  /** Exact visible button text — callers' tests select sort buttons by this label. */
  label: string;
}

export interface SortableStatTableProps<Row, SortKey extends string> {
  /** Pre-sorted by the caller; this component does not sort. */
  rows: Row[];
  rowKey: (row: Row) => string;
  columns: StatTableColumn<Row>[];
  sortKeys: SortKeyOption<SortKey>[];
  sortBy: SortKey;
  onSortByChange: (key: SortKey) => void;
  /** The metric/sample-count/seed summary line, rendered above the sort-by row. */
  summary: ReactNode;
  /** Rendered after </table>, e.g. a pairwise-interactions section. */
  children?: ReactNode;
}

const CELL_PAD = "0.45rem 0.5rem";
const ZEBRA_BG = "rgba(255,255,255,0.03)";

const styles = {
  table: {
    width: "100%",
    borderCollapse: "collapse",
    fontSize: "0.9rem",
    fontVariantNumeric: "tabular-nums",
  },
  headerRow: { borderBottom: "1px solid var(--border)" },
  th: { padding: CELL_PAD },
  td: { padding: CELL_PAD },
  tdHeadline: { padding: CELL_PAD, fontWeight: 600 },
  tdCi: { padding: CELL_PAD, color: "var(--text-muted)", fontSize: "0.85rem" },
  summaryLine: {
    marginBottom: "0.75rem",
    color: "var(--text-muted)",
    fontSize: "0.85rem",
  },
  sortRow: {
    marginBottom: "0.5rem",
    fontSize: "0.85rem",
    color: "var(--text-muted)",
  },
} satisfies Record<string, CSSProperties>;

function sortButtonStyle(active: boolean): CSSProperties {
  return {
    marginRight: "0.5rem",
    padding: "0.15rem 0.5rem",
    border: "1px solid var(--border)",
    background: active ? "var(--accent)" : "transparent",
    color: active ? "var(--bg)" : "inherit",
    borderRadius: 3,
    cursor: "pointer",
    fontSize: "0.8rem",
  };
}

function thStyle(align: ColumnAlign): CSSProperties {
  return { ...styles.th, textAlign: align };
}

function tdStyle(
  variant: ColumnVariant | undefined,
  align: ColumnAlign,
): CSSProperties {
  const base =
    variant === "headline"
      ? styles.tdHeadline
      : variant === "ci"
        ? styles.tdCi
        : styles.td;
  return { ...base, textAlign: align };
}

/**
 * Generic sortable stat table shared by SobolResults/MorrisResults. Purely presentational —
 * sorting stays in the caller since each method's comparators differ per sort key.
 */
export default function SortableStatTable<Row, SortKey extends string>({
  rows,
  rowKey,
  columns,
  sortKeys,
  sortBy,
  onSortByChange,
  summary,
  children,
}: SortableStatTableProps<Row, SortKey>) {
  return (
    <div>
      <div style={styles.summaryLine}>{summary}</div>
      <div style={styles.sortRow}>
        Sort by:{" "}
        {sortKeys.map(({ key, label }) => (
          <button
            key={key}
            type="button"
            onClick={() => onSortByChange(key)}
            style={sortButtonStyle(sortBy === key)}
          >
            {label}
          </button>
        ))}
      </div>
      <table style={styles.table}>
        <thead>
          <tr style={styles.headerRow}>
            {columns.map((col) => (
              <th
                key={col.key}
                style={thStyle(col.align ?? "right")}
                title={col.headerTitle}
              >
                {col.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, i) => (
            <tr
              key={rowKey(row)}
              style={{ background: i % 2 === 1 ? ZEBRA_BG : undefined }}
            >
              {/* One <td> per column, no wrapper — columns[0]'s cell must stay the literal
                  first child of this <tr>; SobolResults/MorrisResults tests read row.children[0]. */}
              {columns.map((col) => (
                <td
                  key={col.key}
                  style={tdStyle(col.variant, col.align ?? "right")}
                >
                  {col.render(row)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {children}
    </div>
  );
}
