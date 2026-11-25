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

  it("evaluates eligibility badges across inferred enemy type and additive Wave Defense", () => {
    const eligibility: api.OfficerEligibilityResponse = {
      scenarios: ["solo_armadas", "red_moving_space", "waves"],
      officer_abilities: {
        trelane: { captain: "ab_waves", officer: [] },
        dezoc: { captain: "ab_solo", officer: [] },
      },
      matrix: {
        ab_waves: {
          ability_id: "ab_waves",
          slot: "captain",
          scenarios: {
            solo_armadas: {
              verdict: "does_not_work",
              reason: "Wave Defense only",
            },
            waves: { verdict: "works" },
          },
        },
        ab_solo: {
          ability_id: "ab_solo",
          slot: "captain",
          scenarios: {
            solo_armadas: { verdict: "works" },
            red_moving_space: {
              verdict: "does_not_work",
              reason: "Solo Armadas only",
            },
            waves: { verdict: "does_not_work", reason: "Solo Armadas only" },
          },
        },
      },
    };

    // Without Wave Defense on a Solo Armada: Dezoc works (no warning), Trelane warns.
    const { rerender } = render(
      <ProfileProvider>
        <WorkspaceModeProvider>
          <CrewBuilder
            belowDecksSlots={1}
            crew={{
              captain: "trelane",
              bridge: ["dezoc", null],
              belowDeck: [null],
            }}
            pins={{
              captain: false,
              bridge: [false, false],
              belowDeck: [false],
            }}
            onCrewChange={vi.fn()}
            onPinsChange={vi.fn()}
            officerOptions={[
              { id: "trelane", name: "Trelane" },
              { id: "dezoc", name: "Dezoc" },
            ]}
            enemyType=""
            inferredEnemyType="solo_armadas"
            waveDefense={false}
            eligibility={eligibility}
          />
        </WorkspaceModeProvider>
      </ProfileProvider>,
    );
    expect(screen.getByText("✕ may not work")).toBeTruthy();

    // Enabling additive Wave Defense lets BOTH Trelane (waves) and Dezoc (solo_armadas) work.
    rerender(
      <ProfileProvider>
        <WorkspaceModeProvider>
          <CrewBuilder
            belowDecksSlots={1}
            crew={{
              captain: "trelane",
              bridge: ["dezoc", null],
              belowDeck: [null],
            }}
            pins={{
              captain: false,
              bridge: [false, false],
              belowDeck: [false],
            }}
            onCrewChange={vi.fn()}
            onPinsChange={vi.fn()}
            officerOptions={[
              { id: "trelane", name: "Trelane" },
              { id: "dezoc", name: "Dezoc" },
            ]}
            enemyType=""
            inferredEnemyType="solo_armadas"
            waveDefense={true}
            eligibility={eligibility}
          />
        </WorkspaceModeProvider>
      </ProfileProvider>,
    );
    expect(screen.queryByText("✕ may not work")).toBeNull();
  });
});
