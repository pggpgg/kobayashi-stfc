import { render, screen } from "@testing-library/react";
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

  function renderBuilder() {
    return render(
      <ProfileProvider>
        <WorkspaceModeProvider>
          <CrewBuilder
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
});
