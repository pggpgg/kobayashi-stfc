import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import type { Preset } from "../lib/api";
import ResultsLibrary from "./ResultsLibrary";

const presetPayload: Preset = {
  id: "p99",
  name: "Loaded",
  ship: "saladin",
  scenario: "2918121098",
  crew: { captain: "c1", bridge: [], below_deck: [] },
  schema_version: 2,
  provenance: {
    saved_at: "2026-01-01T00:00:00Z",
    kobayashi_version: "test",
  },
};

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    fetchProfiles: vi.fn().mockResolvedValue({
      profiles: [{ id: "prof1", name: "Main", sync_token: "t" }],
      default_id: "prof1",
    }),
    fetchPresets: vi.fn(),
    fetchPreset: vi.fn(),
    formatApiError: actual.formatApiError,
  };
});

import * as api from "../lib/api";

function renderLibrary() {
  const router = createMemoryRouter(
    [
      {
        path: "/results",
        element: (
          <ProfileProvider>
            <ResultsLibrary />
          </ProfileProvider>
        ),
      },
      { path: "/", element: <div>Home</div> },
    ],
    { initialEntries: ["/results"] },
  );
  const view = render(<RouterProvider router={router} />);
  return { router, ...view };
}

describe("ResultsLibrary", () => {
  beforeEach(() => {
    vi.mocked(api.fetchPresets).mockReset();
    vi.mocked(api.fetchPreset).mockReset();
  });

  it("shows loading then empty state when no presets", async () => {
    vi.mocked(api.fetchPresets).mockResolvedValue([]);

    renderLibrary();

    expect(screen.getByText("Loading…")).toBeTruthy();

    await waitFor(() => {
      expect(screen.queryByText("Loading…")).toBeNull();
    });

    expect(
      screen.getByText("No saved presets. Save a crew from the Workspace."),
    ).toBeTruthy();
  });

  it("lists presets and navigates to workspace with preset state", async () => {
    vi.mocked(api.fetchPresets).mockResolvedValue([
      {
        id: "a1",
        name: "Fleet",
        ship: "enterprise",
        scenario: "hostile-1",
        schema_version: 2,
      },
    ]);
    vi.mocked(api.fetchPreset).mockResolvedValue(presetPayload);

    const { router } = renderLibrary();

    await waitFor(() => expect(screen.queryByText("Loading…")).toBeNull());

    expect(screen.getByText("Fleet")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Load" }));

    await waitFor(() => {
      expect(api.fetchPreset).toHaveBeenCalledWith("a1", "prof1");
    });

    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/");
    });

    expect(router.state.location.state).toEqual({ preset: presetPayload });
  });

  it("shows API error when fetchPresets fails", async () => {
    vi.mocked(api.fetchPresets).mockRejectedValue(new Error("network down"));

    renderLibrary();

    await waitFor(() => {
      expect(screen.getByText(/network down/)).toBeTruthy();
    });
  });
});
