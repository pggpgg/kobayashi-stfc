import type { ReactNode } from "react";
import type {
  ForbiddenTechCatalogItem,
  ForbiddenTechImportedEntry,
} from "../lib/api";
import { styles } from "../lib/rosterProfileStyles";

/** One equip slot (forbidden tech or chaos tech) — same shape, different data/labels. */
export default function RosterProfileTechSlot({
  title,
  description,
  options,
  catalogByFid,
  equippedFid,
  onChange,
  emptyMessage,
}: {
  title: string;
  description: ReactNode;
  options: ForbiddenTechImportedEntry[];
  catalogByFid: Map<number, ForbiddenTechCatalogItem>;
  equippedFid: number | null | undefined;
  onChange: (fid: number | null) => void;
  emptyMessage: ReactNode;
}) {
  return (
    <>
      <h3
        style={{
          margin: "1.5rem 0 0.5rem",
          fontSize: "0.95rem",
          fontWeight: 600,
        }}
      >
        {title}
      </h3>
      <p
        style={{
          margin: "0 0 0.5rem",
          fontSize: "0.85rem",
          color: "var(--text-muted)",
        }}
      >
        {description}
      </p>
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 8,
          maxWidth: 420,
        }}
      >
        <label style={styles.rowCenter}>
          <span style={{ width: 140 }}>Equipped</span>
          <select
            value={equippedFid != null ? String(equippedFid) : ""}
            onChange={(e) => {
              const v = e.target.value;
              onChange(v === "" ? null : Number.parseInt(v, 10));
            }}
            style={{
              padding: "0.4rem 0.6rem",
              background: "var(--bg)",
              border: "1px solid var(--border)",
              borderRadius: 6,
              color: "var(--text)",
              flex: 1,
            }}
          >
            <option value="">(empty)</option>
            {options.map((e) => (
              <option key={e.fid} value={e.fid}>
                {(catalogByFid.get(e.fid)?.name ?? `fid ${e.fid}`) +
                  ` — T${e.tier} L${e.level}`}
              </option>
            ))}
          </select>
        </label>
        {options.length === 0 && (
          <span
            style={{
              fontSize: "0.85rem",
              color: "var(--text-muted)",
            }}
          >
            {emptyMessage}
          </span>
        )}
      </div>
    </>
  );
}
