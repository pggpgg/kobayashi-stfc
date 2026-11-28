import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { HostileListItem } from "../lib/api";
import HostilePicker from "./HostilePicker";

const hostiles: HostileListItem[] = [
  {
    id: "hostile_a",
    hostile_name: "Borg Probe",
    level: 30,
    ship_class: "interceptor",
  },
  {
    id: "hostile_b",
    hostile_name: "Klingon Bird",
    level: 20,
    ship_class: "explorer",
  },
];

describe("HostilePicker", () => {
  beforeEach(() => {
    Element.prototype.scrollIntoView = vi.fn();
  });

  it("shows level-first selected hostile label with full title when closed", () => {
    render(
      <HostilePicker
        hostiles={hostiles}
        value="hostile_a"
        onChange={vi.fn()}
      />,
    );
    const input = screen.getByRole("combobox") as HTMLInputElement;
    expect(input.value).toBe("Lvl 30 · Borg Probe");
    expect(input.title).toBe("Lvl 30 · Borg Probe");
  });

  it("filters list by name or level and selects on click", () => {
    const onChange = vi.fn();
    render(<HostilePicker hostiles={hostiles} value="" onChange={onChange} />);
    const input = screen.getByRole("combobox");
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: "klingon" } });
    const option = screen.getByRole("option", {
      name: /Lvl 20 · Klingon Bird/,
    });
    fireEvent.mouseDown(option);
    fireEvent.click(option);
    expect(onChange).toHaveBeenCalledWith("hostile_b");
  });

  it("#414 disambiguates duplicate (name, level) hostiles by spawn system/warp and searches by system", () => {
    const duplicateHostiles: HostileListItem[] = [
      {
        id: "3294438884",
        hostile_name: "Xindi-Aquatic Cruiser",
        level: 62,
        ship_class: "interceptor",
        system_names: [],
        warp: 0,
        has_loot: false,
        primary_resource_name: null,
      },
      {
        id: "3919830203",
        hostile_name: "Xindi-Aquatic Cruiser",
        level: 62,
        ship_class: "interceptor",
        system_names: ["Yoton"],
        warp: 1000,
        has_loot: true,
        primary_resource_name: "Tech Module",
      },
    ];
    const onChange = vi.fn();
    render(
      <HostilePicker
        hostiles={duplicateHostiles}
        value="3919830203"
        onChange={onChange}
      />,
    );
    const input = screen.getByRole("combobox") as HTMLInputElement;
    expect(input.value).toBe("Lvl 62 · Xindi-Aquatic Cruiser (Yoton, W1000)");
    expect(input.title).toBe("Lvl 62 · Xindi-Aquatic Cruiser (Yoton, W1000)");

    fireEvent.focus(input);
    expect(
      screen.getByRole("option", {
        name: "Lvl 62 · Xindi-Aquatic Cruiser (no system, no loot · #3294438884)",
      }),
    ).toBeTruthy();

    fireEvent.change(input, { target: { value: "yoton" } });
    const options = screen.getAllByRole("option");
    expect(options).toHaveLength(1);
    expect(options[0].textContent).toBe(
      "Lvl 62 · Xindi-Aquatic Cruiser (Yoton, W1000)",
    );
  });
});
