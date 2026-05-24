import { type ReactNode, useEffect, useState } from "react";

interface Props {
  /** Unique key for persisting open/closed state in localStorage. */
  storageKey: string;
  /** Visible title (e.g. "How to read this"). */
  title: string;
  /** Default open state when no localStorage value exists. */
  defaultOpen?: boolean;
  children: ReactNode;
}

/**
 * Reusable expandable explainer panel for plain-language method descriptions on the
 * Sensitivity page. Open/closed state is persisted per `storageKey` in localStorage so
 * power users don't see the same explanation re-opening on every page revisit.
 */
export default function ExplainerPanel({
  storageKey,
  title,
  defaultOpen = false,
  children,
}: Props) {
  const [open, setOpen] = useState<boolean>(() => {
    if (typeof window === "undefined") return defaultOpen;
    const stored = window.localStorage.getItem(`explainer:${storageKey}`);
    if (stored === null) return defaultOpen;
    return stored === "1";
  });

  useEffect(() => {
    if (typeof window === "undefined") return;
    window.localStorage.setItem(`explainer:${storageKey}`, open ? "1" : "0");
  }, [open, storageKey]);

  return (
    <details
      open={open}
      onToggle={(e) => setOpen((e.target as HTMLDetailsElement).open)}
      style={{
        marginBottom: "1rem",
        padding: "0.5rem 0.75rem",
        border: "1px solid var(--border)",
        borderRadius: 6,
        background: "rgba(255,255,255,0.02)",
      }}
    >
      <summary
        style={{
          cursor: "pointer",
          fontWeight: 600,
          fontSize: "0.9rem",
          color: "var(--text-muted)",
          userSelect: "none",
        }}
      >
        {title}
      </summary>
      <div
        style={{
          marginTop: "0.75rem",
          fontSize: "0.88rem",
          lineHeight: 1.55,
          color: "var(--text)",
        }}
      >
        {children}
      </div>
    </details>
  );
}
