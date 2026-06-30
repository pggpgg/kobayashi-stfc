import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import SortableStatTable, { type StatTableColumn } from "./SortableStatTable";

interface TestRow {
  stat: string;
  value: number;
}

const rows: TestRow[] = [
  { stat: "alpha", value: 1 },
  { stat: "beta", value: 2 },
];

const columns: StatTableColumn<TestRow>[] = [
  { key: "stat", header: "Stat", align: "left", render: (r) => r.stat },
  {
    key: "value",
    header: "Value",
    variant: "headline",
    render: (r) => String(r.value),
  },
];

const sortKeys = [
  { key: "value" as const, label: "Value" },
  { key: "stat" as const, label: "Stat" },
];

function renderTable(onSortByChange = vi.fn()) {
  return render(
    <SortableStatTable
      rows={rows}
      rowKey={(r) => r.stat}
      columns={columns}
      sortKeys={sortKeys}
      sortBy="value"
      onSortByChange={onSortByChange}
      summary="N=2 samples"
    />,
  );
}

describe("SortableStatTable", () => {
  it("renders the summary and sort buttons with exact label text", () => {
    renderTable();
    expect(screen.getByText("N=2 samples")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Value" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Stat" })).toBeTruthy();
  });

  it("reports the clicked sort key to the caller", () => {
    const onSortByChange = vi.fn();
    renderTable(onSortByChange);
    fireEvent.click(screen.getByRole("button", { name: "Stat" }));
    expect(onSortByChange).toHaveBeenCalledWith("stat");
  });

  it("keeps the first column's cell as the literal first child of each body row", () => {
    renderTable();
    const statCells = screen
      .getAllByRole("row")
      .slice(1)
      .map((row) => row.children[0]?.textContent);
    expect(statCells).toEqual(["alpha", "beta"]);
  });

  it("applies a zebra background to odd rows only", () => {
    renderTable();
    const bodyRows = screen.getAllByRole("row").slice(1);
    expect(bodyRows[0]?.style.background).toBe("");
    expect(bodyRows[1]?.style.background).toContain(
      "rgba(255, 255, 255, 0.03)",
    );
  });

  it("renders children after the table", () => {
    render(
      <SortableStatTable
        rows={rows}
        rowKey={(r) => r.stat}
        columns={columns}
        sortKeys={sortKeys}
        sortBy="value"
        onSortByChange={() => {}}
        summary="N=2 samples"
      >
        <div data-testid="extra-section">extra</div>
      </SortableStatTable>,
    );
    expect(screen.getByTestId("extra-section")).toBeTruthy();
  });
});
