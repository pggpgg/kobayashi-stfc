import type { ProfileEntry } from "../lib/api";
import { styles } from "../lib/rosterProfileStyles";

export default function RosterProfileAttributesCard({
  activeProfile,
}: {
  activeProfile: ProfileEntry;
}) {
  return (
    <>
      <h2 style={{ margin: "0 0 1rem", fontSize: "1rem", fontWeight: 600 }}>
        Player profile attributes
      </h2>
      <dl
        style={{
          margin: 0,
          display: "grid",
          gap: "0.75rem 1rem",
          gridTemplateColumns: "auto 1fr",
          maxWidth: 560,
        }}
      >
        <dt style={{ color: "var(--text-muted)", fontWeight: 500 }}>Name</dt>
        <dd style={styles.noMargin}>{activeProfile.name}</dd>

        <dt style={{ color: "var(--text-muted)", fontWeight: 500 }}>
          Profile ID
        </dt>
        <dd style={styles.noMargin}>
          <code
            style={{
              padding: "0.2rem 0.4rem",
              background: "var(--bg)",
              borderRadius: 4,
              fontSize: "0.85rem",
              fontFamily: "monospace",
            }}
          >
            {activeProfile.id}
          </code>
        </dd>

        <dt style={{ color: "var(--text-muted)", fontWeight: 500 }}>
          Sync token (UUID)
        </dt>
        <dd
          style={{
            margin: 0,
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}
        >
          <code
            style={{
              padding: "0.35rem 0.5rem",
              background: "var(--bg)",
              borderRadius: 4,
              fontSize: "0.8rem",
              fontFamily: "monospace",
              wordBreak: "break-all",
            }}
          >
            {activeProfile.sync_token}
          </code>
          <button
            type="button"
            onClick={() =>
              navigator.clipboard.writeText(activeProfile.sync_token)
            }
            style={{
              padding: "0.35rem 0.6rem",
              background: "var(--accent)",
              border: "none",
              borderRadius: 4,
              color: "var(--bg)",
              fontSize: "0.8rem",
              cursor: "pointer",
              flexShrink: 0,
            }}
          >
            Copy
          </button>
        </dd>
      </dl>
      <p
        style={{
          marginTop: "1rem",
          marginBottom: "0.75rem",
          fontSize: "0.85rem",
          color: "var(--text-muted)",
        }}
      >
        Add this to your <code>community_patch_settings.toml</code> to sync
        stfc-mod data to this profile:
      </p>
      <div
        style={{
          position: "relative",
          background: "var(--bg)",
          border: "1px solid var(--border)",
          borderRadius: 6,
          padding: "1rem",
          fontFamily: "monospace",
          fontSize: "0.85rem",
          overflow: "auto",
        }}
      >
        <pre
          style={{
            margin: 0,
            paddingRight: 60,
            whiteSpace: "pre-wrap",
            wordBreak: "break-all",
          }}
        >
          {`[sync.targets.kobayashi-${activeProfile.id}]
url = "http://localhost:3000/api/sync/ingress"
token = "${activeProfile.sync_token}"`}
        </pre>
        <button
          type="button"
          onClick={() =>
            navigator.clipboard.writeText(
              `[sync.targets.kobayashi-${activeProfile.id}]\nurl = "http://localhost:3000/api/sync/ingress"\ntoken = "${activeProfile.sync_token}"`,
            )
          }
          style={{
            position: "absolute",
            top: 8,
            right: 8,
            padding: "0.35rem 0.6rem",
            background: "var(--accent)",
            border: "none",
            borderRadius: 4,
            color: "var(--bg)",
            fontSize: "0.8rem",
            cursor: "pointer",
          }}
        >
          Copy
        </button>
      </div>
    </>
  );
}
