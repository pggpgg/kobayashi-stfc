import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { OfficerListItem } from "../lib/api";
import OfficerNameMultiSelect from "./OfficerNameMultiSelect";

const officers: OfficerListItem[] = [
  { id: "kirk", name: "Kirk" },
  { id: "spock", name: "Spock" },
];

describe("OfficerNameMultiSelect", () => {
  it("renders label and selected chips", () => {
    render(
      <OfficerNameMultiSelect
        label="Must include"
        valueComma="Kirk"
        onChangeComma={vi.fn()}
        officers={officers}
      />,
    );
    expect(screen.getByText("Must include")).toBeTruthy();
    expect(screen.getByText("Kirk")).toBeTruthy();
  });

  it("adds officer from suggestion list", () => {
    const onChange = vi.fn();
    render(
      <OfficerNameMultiSelect
        label="Pool"
        valueComma=""
        onChangeComma={onChange}
        officers={officers}
      />,
    );
    const input = screen.getByRole("combobox");
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: "Spock" } });
    fireEvent.click(screen.getByRole("option", { name: "Spock" }));
    expect(onChange).toHaveBeenCalledWith("Spock");
  });
});
