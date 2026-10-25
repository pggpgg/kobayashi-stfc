import { type CSSProperties, type ReactNode, useEffect, useRef } from "react";
import type {
  OptimizerSettings,
  UpdateOptimizerSettings,
} from "../../lib/useOptimizerSettings";
export const selectStyle: CSSProperties = {
  display: "block",
  marginTop: "var(--space-1)",
  width: "100%",
  padding: "var(--space-5)",
  background: "var(--bg)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-1)",
  color: "var(--text)",
};

/** Same field surface as `selectStyle`, with tighter padding for text inputs. */
export const inputStyleCompact: CSSProperties = {
  ...selectStyle,
  padding: "var(--space-3)",
};

/** Bare chevron button used to collapse/expand the panel. */
export const iconButtonStyle: CSSProperties = {
  padding: "var(--space-1)",
  background: "transparent",
  border: "none",
  color: "var(--text-muted)",
};

/** Drop the default checkbox margin so it aligns inside flex label rows. */
export const checkboxResetStyle: CSSProperties = { margin: 0 };

export const checkboxLabelStyle: CSSProperties = {
  fontSize: "0.85rem",
  display: "flex",
  alignItems: "center",
  gap: "var(--space-8)",
  cursor: "pointer",
};

export const labelWithHintRowStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "var(--space-1)",
  marginBottom: "var(--space-1)",
};

export function HelpHint({ text }: { text: string }) {
  return (
    <span
      title={text}
      role="note"
      aria-label={text}
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        width: 14,
        height: 14,
        fontSize: 10,
        fontWeight: 700,
        lineHeight: 1,
        borderRadius: "50%",
        border: "1px solid var(--border)",
        color: "var(--text-muted)",
        cursor: "help",
        flexShrink: 0,
      }}
    >
      ?
    </span>
  );
}

export function FieldLabelWithHint({
  children,
  hint,
}: {
  children: ReactNode;
  hint: string;
}) {
  return (
    <div style={labelWithHintRowStyle}>
      {children}
      <HelpHint text={hint} />
    </div>
  );
}

export const sectionSummaryStyle: CSSProperties = {
  cursor: "pointer",
  display: "flex",
  alignItems: "center",
  gap: "var(--space-4)",
  fontSize: "0.85rem",
  fontWeight: 600,
};

/**
 * Advanced controls grouped under a native `<details>` so the common optimize
 * path (seeds + strategy) stays uncluttered. Collapsed by default; the browser
 * preserves each section's open state across re-renders.
 */
export function CollapsibleSection({
  title,
  hint,
  children,
  initiallyOpen,
}: {
  initiallyOpen?: boolean;
  title: string;
  hint?: string;
  children: ReactNode;
}) {
  const details = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    if (initiallyOpen && details.current) details.current.open = true;
  }, [initiallyOpen]);
  return (
    <details className="opt-section" ref={details}>
      <summary style={sectionSummaryStyle}>
        <span className="opt-caret" aria-hidden="true">
          ▸
        </span>
        <span>{title}</span>
        {hint ? <HelpHint text={hint} /> : null}
      </summary>
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "0.75rem",
          marginTop: "0.75rem",
        }}
      >
        {children}
      </div>
    </details>
  );
}

export function settingUpdater(onChange: UpdateOptimizerSettings) {
  return <K extends keyof OptimizerSettings>(key: K) =>
    (value: OptimizerSettings[K]) =>
      onChange({ [key]: value });
}
