import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import LoopsWorkspace from "./LoopsWorkspace";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    fetchProfiles: vi.fn().mockResolvedValue({
      profiles: [{ id: "p1", name: "Main", sync_token: "token" }],
      default_id: "p1",
    }),
    fetchHostiles: vi.fn().mockResolvedValue([
      {
        id: "actian-40",
        hostile_name: "Actian Instigator",
        level: 40,
        ship_class: "explorer",
      },
      {
        id: "actian-49",
        hostile_name: "Actian Apex",
        level: 49,
        ship_class: "interceptor",
      },
    ]),
    fetchShips: vi.fn().mockResolvedValue([
      { id: "mantis", ship_name: "MANTIS", ship_class: "battleship" },
      { id: "saladin", ship_name: "SALADIN", ship_class: "interceptor" },
    ]),
  };
});

function WorkspaceStateProbe() {
  const location = useLocation();
  const state = location.state as {
    loopRun?: { loopId: string; targetId: string };
  } | null;
  return (
    <div>
      handoff:{state?.loopRun?.loopId}:{state?.loopRun?.targetId}
    </div>
  );
}

describe("LoopsWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
  });

  it("renders a high-to-low ladder and hands a rung to the optimizer", async () => {
    render(
      <MemoryRouter initialEntries={["/loops"]}>
        <ProfileProvider>
          <Routes>
            <Route path="/loops" element={<LoopsWorkspace />} />
            <Route path="/" element={<WorkspaceStateProbe />} />
          </Routes>
        </ProfileProvider>
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Actian" }));
    await screen.findByText("Actian Apex");
    const levels = screen
      .getAllByText(/^(49|40)$/)
      .map((node) => node.textContent);
    expect(levels).toEqual(["49", "40"]);

    const optimizeButtons = screen.getAllByRole("button", { name: "Optimize" });
    fireEvent.click(optimizeButtons[0]);
    await waitFor(() => {
      expect(screen.getByText("handoff:actian:actian-49")).toBeTruthy();
    });
  });
});
