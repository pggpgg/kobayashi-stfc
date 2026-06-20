import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import { WorkspaceModeProvider } from "../contexts/WorkspaceModeContext";
import Shell from "./Shell";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return { ...actual, fetchProfiles: vi.fn() };
});

import * as api from "../lib/api";

describe("Shell", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(api.fetchProfiles).mockResolvedValue({
      profiles: [{ id: "p1", name: "Test", sync_token: "t" }],
      default_id: "p1",
    });
  });

  it("renders nav links and child content", () => {
    render(
      <MemoryRouter initialEntries={["/"]}>
        <ProfileProvider>
          <WorkspaceModeProvider>
            <Shell>
              <div>Main content</div>
            </Shell>
          </WorkspaceModeProvider>
        </ProfileProvider>
      </MemoryRouter>,
    );
    expect(screen.getByText("Workspace")).toBeTruthy();
    expect(screen.getByText("Main content")).toBeTruthy();
    expect(screen.getByRole("link", { name: "PvP" }).getAttribute("href")).toBe(
      "/pvp",
    );
  });

  it("persists guided mode", () => {
    render(
      <MemoryRouter initialEntries={["/"]}>
        <ProfileProvider>
          <WorkspaceModeProvider>
            <Shell>
              <div>Main content</div>
            </Shell>
          </WorkspaceModeProvider>
        </ProfileProvider>
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Guided" }));
    expect(localStorage.getItem("kobayashi_workspace_mode")).toBe("guided");
  });
});
