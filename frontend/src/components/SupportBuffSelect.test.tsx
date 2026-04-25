import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { SupportBuffId } from "../lib/supportBuffs";
import SupportBuffSelect from "./SupportBuffSelect";

describe("SupportBuffSelect", () => {
  it("groups support buffs by source with stat category chips", () => {
    render(<SupportBuffSelect selected={[]} onChange={vi.fn()} />);

    expect(screen.getByText("Titan-A Fortify")).toBeTruthy();
    expect(screen.getByText("Cerritos")).toBeTruthy();
    expect(screen.getByText("Defiant")).toBeTruthy();
    expect(screen.getByText("Only one Fortify level can apply.")).toBeTruthy();

    const fortifyGroup = screen.getByText("Titan-A Fortify").closest("section");
    expect(fortifyGroup).toBeTruthy();
    expect(
      within(fortifyGroup as HTMLElement).getByText("Max fortification"),
    ).toBeTruthy();
    expect(
      within(fortifyGroup as HTMLElement).getByText("Fortification"),
    ).toBeTruthy();
    expect(
      within(fortifyGroup as HTMLElement).getAllByText("Crit damage").length,
    ).toBeGreaterThan(0);
    expect(
      within(fortifyGroup as HTMLElement).getAllByText("Weapon damage").length,
    ).toBeGreaterThan(0);

    const defiantGroup = screen.getByText("Defiant").closest("section");
    expect(defiantGroup).toBeTruthy();
    expect(
      within(defiantGroup as HTMLElement).getByText("Research gate"),
    ).toBeTruthy();
  });

  it("toggles selected support buff ids", () => {
    const onChange = vi.fn();
    render(
      <SupportBuffSelect selected={["cerritos_support"]} onChange={onChange} />,
    );

    fireEvent.click(
      screen.getByRole("checkbox", { name: /Cerritos Support/i }),
    );
    expect(onChange).toHaveBeenCalledWith([]);

    fireEvent.click(
      screen.getByRole("checkbox", { name: /Defiant Reinforce/i }),
    );
    expect(onChange).toHaveBeenCalledWith([
      "cerritos_support",
      "defiant_reinforce",
    ] satisfies SupportBuffId[]);
  });

  it("normalizes incompatible selections when toggling", () => {
    const onChange = vi.fn();
    render(
      <SupportBuffSelect
        selected={["titan_a_fortification"]}
        onChange={onChange}
      />,
    );

    fireEvent.click(
      screen.getByRole("checkbox", { name: /Max fortification/i }),
    );
    expect(onChange).toHaveBeenCalledWith([
      "titan_a_max_fortification",
    ] satisfies SupportBuffId[]);
  });

  it("surfaces validation feedback for unsupported or duplicate selections", () => {
    render(
      <SupportBuffSelect
        selected={["unknown_buff", "cerritos_support", "cerritos_support"]}
        onChange={vi.fn()}
      />,
    );

    expect(screen.getByRole("status").textContent).toMatch(
      /Unsupported support buff/,
    );
    expect(screen.getByText("Support buffs (1)")).toBeTruthy();
  });
});
