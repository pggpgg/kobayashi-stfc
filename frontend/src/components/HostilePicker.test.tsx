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

  it("shows selected hostile label when closed", () => {
    render(
      <HostilePicker
        hostiles={hostiles}
        value="hostile_a"
        onChange={vi.fn()}
      />,
    );
    const input = screen.getByRole("combobox") as HTMLInputElement;
    expect(input.value).toBe("Borg Probe (Lvl 30)");
  });

  it("filters list and selects on click", () => {
    const onChange = vi.fn();
    render(<HostilePicker hostiles={hostiles} value="" onChange={onChange} />);
    const input = screen.getByRole("combobox");
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: "klingon" } });
    const option = screen.getByRole("option", { name: /Klingon Bird/ });
    fireEvent.mouseDown(option);
    fireEvent.click(option);
    expect(onChange).toHaveBeenCalledWith("hostile_b");
  });
});
