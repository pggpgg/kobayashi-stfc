import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import type { ImportReport, PlayerProfile } from "../lib/api";
import RosterProfile from "./RosterProfile";

const profileFixture: PlayerProfile = {
  bonuses: { weapon: 5, shield: 2 },
};

const importReportFixture: ImportReport = {
  source_path: "paste",
  output_path: "profiles/p1/roster.imported.json",
  total_records: 3,
  matched_records: 2,
  unmatched_records: 1,
  roster_entries_written: 2,
  unresolved: [
    {
      record_index: 2,
      input_name: "Unknown Officer",
      reason: "no match",
      suggested_matches: ["Kirk"],
    },
  ],
};

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    fetchProfiles: vi.fn(),
    fetchProfile: vi.fn(),
    fetchForbiddenTech: vi.fn(),
    fetchForbiddenTechImported: vi.fn(),
    fetchBuildingCombatSummary: vi.fn(),
    fetchResearchCombatSummary: vi.fn(),
    fetchModSyncStatus: vi.fn(),
    importRoster: vi.fn(),
    updateProfile: vi.fn(),
    formatApiError: actual.formatApiError,
  };
});

import * as api from "../lib/api";

function renderPage() {
  return render(
    <ProfileProvider>
      <RosterProfile />
    </ProfileProvider>,
  );
}

describe("RosterProfile", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();

    vi.mocked(api.fetchProfiles).mockResolvedValue({
      profiles: [{ id: "p1", name: "HiggsBozo", sync_token: "sync-token-1" }],
      default_id: "p1",
    });
    vi.mocked(api.fetchProfile).mockResolvedValue(profileFixture);
    vi.mocked(api.fetchForbiddenTech).mockResolvedValue([]);
    vi.mocked(api.fetchForbiddenTechImported).mockResolvedValue({
      profile_id: "p1",
      forbidden_tech: [],
    });
    vi.mocked(api.fetchBuildingCombatSummary).mockResolvedValue({
      profile_id: "p1",
      synced_building_count: 0,
      buildings: [],
      unmapped_bids: [],
    });
    vi.mocked(api.fetchResearchCombatSummary).mockResolvedValue({
      profile_id: "p1",
      synced_research_count: 0,
      research: [],
      unmapped_rids: [],
    });
    vi.mocked(api.fetchModSyncStatus).mockResolvedValue({
      profile_id: "p1",
      last_mod_sync_utc: null,
    });
    vi.mocked(api.importRoster).mockResolvedValue(importReportFixture);
    vi.mocked(api.updateProfile).mockResolvedValue(undefined);
  });

  it("shows the active profile on the profile tab and mod-sync guidance", async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByText(/Roster & Profile/)).toBeTruthy();
      expect(screen.getByText("(HiggsBozo)")).toBeTruthy();
    });

    expect(
      screen.getByText(/No community mod sync recorded yet for this profile/),
    ).toBeTruthy();
    expect(screen.getByText("Player profile attributes")).toBeTruthy();
    expect(screen.getByText("sync-token-1")).toBeTruthy();
  });

  it("shows mod sync and building summary errors", async () => {
    vi.mocked(api.fetchModSyncStatus).mockRejectedValue(new Error("sync offline"));
    vi.mocked(api.fetchBuildingCombatSummary).mockRejectedValue(
      new Error("buildings unavailable"),
    );

    renderPage();

    await waitFor(() => {
      expect(screen.getByText(/sync offline/)).toBeTruthy();
      expect(screen.getByText(/buildings unavailable/)).toBeTruthy();
    });
  });

  it("imports roster paste and surfaces unresolved names", async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByText("(HiggsBozo)")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: "Roster Import" }));
    fireEvent.change(screen.getByPlaceholderText("Paste JSON or CSV here..."), {
      target: { value: "Kirk,1,10" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Import" }));

    await waitFor(() => {
      expect(api.importRoster).toHaveBeenCalledWith("Kirk,1,10", "p1");
    });

    expect(screen.getByText(/Matched: 2, written: 2/)).toBeTruthy();
    expect(screen.getByText(/Unresolved names/)).toBeTruthy();
    expect(screen.getByText(/Unknown Officer/)).toBeTruthy();
    expect(screen.getByText(/Similar canonical names: Kirk/)).toBeTruthy();
  });

  it("shows import errors from the API", async () => {
    vi.mocked(api.importRoster).mockRejectedValue(new Error("invalid roster"));

    renderPage();

    await waitFor(() => {
      expect(screen.getByText("(HiggsBozo)")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: "Roster Import" }));
    fireEvent.change(screen.getByPlaceholderText("Paste JSON or CSV here..."), {
      target: { value: "bad data" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Import" }));

    await waitFor(() => {
      expect(screen.getByText(/invalid roster/)).toBeTruthy();
    });
  });

  it("edits bonuses, saves profile, and shows save errors", async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByText("(HiggsBozo)")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: "Player Bonuses" }));

    const saveButton = screen.getByRole("button", { name: "Save profile" });
    expect((saveButton as HTMLButtonElement).disabled).toBe(true);

    const weaponInput = screen.getByDisplayValue("5");
    fireEvent.change(weaponInput, { target: { value: "7.5" } });

    await waitFor(() => {
      expect((saveButton as HTMLButtonElement).disabled).toBe(false);
    });

    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(api.updateProfile).toHaveBeenCalledWith(
        expect.objectContaining({
          bonuses: expect.objectContaining({ weapon: 7.5 }),
          forbidden_tech_override: null,
          chaos_tech_override: null,
        }),
        "p1",
      );
    });

    vi.mocked(api.updateProfile).mockRejectedValueOnce(new Error("save failed"));
    fireEvent.change(weaponInput, { target: { value: "8" } });
    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(screen.getByText(/save failed/)).toBeTruthy();
    });
  });
});
