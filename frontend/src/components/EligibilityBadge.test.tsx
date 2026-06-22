import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { EligibilityBadge } from "./EligibilityBadge";

describe("EligibilityBadge", () => {
  it("renders nothing for works or null", () => {
    const { container: a } = render(<EligibilityBadge verdict="works" />);
    expect(a.firstChild).toBeNull();
    const { container: b } = render(<EligibilityBadge verdict={null} />);
    expect(b.firstChild).toBeNull();
  });

  it("renders a warning chip with the reason for does_not_work", () => {
    render(<EligibilityBadge verdict="does_not_work" reason="EnemyPlayer" />);
    const el = screen.getByText(/may not work/i);
    expect(el).toBeTruthy();
    expect(el.getAttribute("title")).toContain("EnemyPlayer");
  });

  it("renders a conditional chip with the reason", () => {
    render(<EligibilityBadge verdict="conditional" reason="SelfHasMorale" />);
    const el = screen.getByText(/conditional/i);
    expect(el).toBeTruthy();
    expect(el.getAttribute("title")).toContain("SelfHasMorale");
  });
});
