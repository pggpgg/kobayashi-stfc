import { type RefObject, useLayoutEffect } from "react";

const FOCUSABLE_SELECTOR =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusableIn(container: HTMLElement): HTMLElement[] {
  return Array.from(
    container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
  ).filter((el) => el.offsetParent !== null || el.getClientRects().length > 0);
}

/** When `active`, moves focus into `containerRef` and traps Tab within it; restores focus on close. */
export function useModalFocusTrap(
  active: boolean,
  containerRef: RefObject<HTMLElement | null>,
) {
  useLayoutEffect(() => {
    if (!active) return;
    const previous = document.activeElement as HTMLElement | null;
    const root = containerRef.current;
    if (!root) return;

    const nodes = focusableIn(root);
    const first = nodes[0] ?? root;
    first.focus();

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Tab" || nodes.length === 0) return;
      const f = nodes[0];
      const l = nodes[nodes.length - 1];
      if (!f || !l) return;
      if (e.shiftKey) {
        if (document.activeElement === f) {
          e.preventDefault();
          l.focus();
        }
      } else if (document.activeElement === l) {
        e.preventDefault();
        f.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      previous?.focus?.();
    };
  }, [active, containerRef]);
}
