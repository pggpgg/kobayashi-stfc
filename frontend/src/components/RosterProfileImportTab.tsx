import type { ImportReport } from "../lib/api";
import { styles } from "../lib/rosterProfileStyles";

export default function RosterProfileImportTab({
  paste,
  onPasteChange,
  onImport,
  importError,
  importResult,
}: {
  paste: string;
  onPasteChange: (value: string) => void;
  onImport: () => void;
  importError: string | null;
  importResult: ImportReport | null;
}) {
  return (
    <section
      style={{
        padding: "1rem",
        background: "var(--surface)",
        border: "1px solid var(--border)",
        borderRadius: 8,
      }}
    >
      <p
        style={{
          margin: "0 0 0.5rem",
          fontSize: "0.9rem",
          color: "var(--text-muted)",
        }}
      >
        Paste Spocks.club export (JSON) or CSV (name,tier,level per line).
      </p>
      <textarea
        value={paste}
        onChange={(e) => onPasteChange(e.target.value)}
        placeholder="Paste JSON or CSV here..."
        rows={12}
        style={{
          width: "100%",
          padding: 8,
          background: "var(--bg)",
          border: "1px solid var(--border)",
          borderRadius: 6,
          color: "var(--text)",
          fontFamily: "monospace",
          fontSize: "0.85rem",
        }}
      />
      <button
        type="button"
        onClick={onImport}
        style={{
          marginTop: 8,
          padding: "0.5rem 1rem",
          background: "var(--accent)",
          border: "none",
          borderRadius: 6,
          color: "var(--bg)",
        }}
      >
        Import
      </button>
      {importError && <div style={styles.errorNote}>{importError}</div>}
      {importResult && (
        <div
          style={{
            marginTop: 12,
            padding: 8,
            background: "var(--bg)",
            borderRadius: 6,
          }}
        >
          <strong>Import result</strong>
          <div>
            Matched: {importResult.matched_records}, written:{" "}
            {importResult.roster_entries_written}
            {importResult.critical_failures != null &&
              importResult.critical_failures > 0 && (
                <span style={{ color: "var(--error)", marginLeft: 8 }}>
                  ({importResult.critical_failures} blocking issue
                  {importResult.critical_failures === 1 ? "" : "s"})
                </span>
              )}
          </div>
          {importResult.diagnostics && importResult.diagnostics.length > 0 && (
            <div style={{ marginTop: 8, fontSize: "0.85rem" }}>
              <strong style={styles.muted}>Warnings (tier / level)</strong>
              <ul style={{ margin: "4px 0 0", paddingLeft: 18 }}>
                {importResult.diagnostics.map((d, i) => (
                  <li key={i} style={{ marginBottom: 4 }}>
                    Row {d.record_index + 1} ({d.input_name}): {d.message}
                    {d.hint && (
                      <div
                        style={{
                          color: "var(--text-muted)",
                          marginTop: 2,
                        }}
                      >
                        {d.hint}
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {importResult.unresolved && importResult.unresolved.length > 0 && (
            <div style={{ marginTop: 8, fontSize: "0.85rem" }}>
              <strong style={{ color: "var(--error)" }}>
                Unresolved names
              </strong>
              <ul style={{ margin: "4px 0 0", paddingLeft: 18 }}>
                {importResult.unresolved.map((u, i) => (
                  <li key={i} style={{ marginBottom: 6 }}>
                    Row {u.record_index + 1}: &quot;{u.input_name}&quot; —{" "}
                    {u.reason}
                    {u.suggested_matches && u.suggested_matches.length > 0 && (
                      <div style={{ marginTop: 2 }}>
                        Similar canonical names:{" "}
                        {u.suggested_matches.join(", ")}
                      </div>
                    )}
                    {u.hint && (
                      <div
                        style={{
                          color: "var(--text-muted)",
                          marginTop: 2,
                        }}
                      >
                        {u.hint}
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
