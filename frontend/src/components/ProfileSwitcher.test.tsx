import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import ProfileSwitcher from "./ProfileSwitcher";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    fetchProfiles: vi.fn(),
    createProfile: vi.fn(),
    deleteProfile: vi.fn(),
  };
});

import * as api from "../lib/api";

describe("ProfileSwitcher", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(api.fetchProfiles).mockResolvedValue({
      profiles: [
        { id: "a", name: "Alpha", sync_token: "t1" },
        { id: "b", name: "Beta", sync_token: "t2" },
      ],
      default_id: "a",
    });
    vi.mocked(api.createProfile).mockResolvedValue({
      id: "new",
      name: "Gamma",
      sync_token: "t3",
    });
    vi.mocked(api.deleteProfile).mockResolvedValue(undefined);
  });

  function renderSwitcher() {
    return render(
      <ProfileProvider>
        <ProfileSwitcher />
      </ProfileProvider>,
    );
  }

  it("opens menu and switches active profile", async () => {
    renderSwitcher();

    await waitFor(() => {
      expect(screen.getByTitle("Alpha")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: "AL" }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "BE Beta" })).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: "BE Beta" }));

    await waitFor(() => {
      expect(localStorage.getItem("kobayashi_active_profile")).toBe("b");
    });
  });

  it("creates a profile from add flow", async () => {
    renderSwitcher();

    await waitFor(() => {
      expect(screen.getByTitle("Alpha")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: "AL" }));
    fireEvent.click(screen.getByRole("button", { name: "+ Add profile" }));

    fireEvent.change(screen.getByPlaceholderText("Profile name"), {
      target: { value: "Gamma" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() => {
      expect(api.createProfile).toHaveBeenCalledWith({ name: "Gamma" });
    });

    await waitFor(() => {
      expect(localStorage.getItem("kobayashi_active_profile")).toBe("new");
    });
  });
});
