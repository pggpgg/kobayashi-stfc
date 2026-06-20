import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import { WorkspaceModeProvider } from "../contexts/WorkspaceModeContext";
import type { CrewState } from "../lib/types";
import CrewBuilder from "./CrewBuilder";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    fetchOfficers: vi.fn(),
    fetchProfiles: vi.fn(),
  };
});

import * as api from "../lib/api";

const emptyCrew: CrewState = {
  captain: null,
  bridge: [null, null],
  belowDeck: [null, null, null],
};

describe("CrewBuilder", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(api.fetchProfiles).mockResolvedValue({
      profiles: [{ id: "p1", name: "Test", sync_token: "t" }],
      default_id: "p1",
    });
  });

  function renderBuilder(guided = false) {
    return render(
      <ProfileProvider>
        <WorkspaceModeProvider>
          <CrewBuilder
            guided={guided}
            belowDecksSlots={2}
            crew={emptyCrew}
            pins={{ captain: false, bridge: [false, false], belowDeck: [] }}
            onCrewChange={vi.fn()}
            onPinsChange={vi.fn()}
            officerOptions={[
              { id: "kirk", name: "Kirk" },
              { id: "spock", name: "Spock" },
            ]}
          />
        </WorkspaceModeProvider>
      </ProfileProvider>,
    );
  }

  it("renders captain slot label", () => {
    renderBuilder();
    expect(screen.getByText(/captain/i)).toBeTruthy();
  });

  it("progressively reveals optional below-deck slots in guided mode", () => {
    renderBuilder(true);

    expect(screen.getByText("Choose your bridge crew")).toBeTruthy();
    expect(screen.getAllByRole("combobox")).toHaveLength(3);
    expect(screen.queryByRole("button", { name: "Pin" })).toBeNull();

    fireEvent.click(
      screen.getByRole("button", {
        name: "Add optional below-deck officers (2 slots)",
      }),
    );

    expect(screen.getAllByRole("combobox")).toHaveLength(5);
  });
});
