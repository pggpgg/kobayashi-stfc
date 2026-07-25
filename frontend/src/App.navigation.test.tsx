import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import App from "./App";
import { ProfileProvider } from "./contexts/ProfileContext";
import { WorkspaceModeProvider } from "./contexts/WorkspaceModeContext";

vi.mock("./pages/Workspace", () => ({
  default: () => <div>WorkspacePageMarker</div>,
}));
vi.mock("./pages/ResultsLibrary", () => ({
  default: () => <div>ResultsLibraryPageMarker</div>,
}));
vi.mock("./pages/LoopsWorkspace", () => ({
  default: () => <div>LoopsWorkspacePageMarker</div>,
}));
vi.mock("./pages/RosterProfile", () => ({
  default: () => <div>RosterProfilePageMarker</div>,
}));
vi.mock("./pages/DataMechanics", () => ({
  default: () => <div>DataMechanicsPageMarker</div>,
}));
vi.mock("./pages/Learn", () => ({
  default: () => <div>LearnPageMarker</div>,
}));

vi.mock("./lib/api", () => ({
  fetchProfiles: vi.fn().mockResolvedValue({
    profiles: [{ id: "p1", name: "Main", sync_token: "t" }],
    default_id: "p1",
  }),
}));

function renderApp(initialPath = "/") {
  return render(
    <MemoryRouter initialEntries={[initialPath]}>
      <WorkspaceModeProvider>
        <ProfileProvider>
          <App />
        </ProfileProvider>
      </WorkspaceModeProvider>
    </MemoryRouter>,
  );
}

describe("App navigation", () => {
  it("renders workspace on /", () => {
    renderApp("/");
    expect(screen.getByText("WorkspacePageMarker")).toBeTruthy();
  });

  it("navigates to Loops, Results Library, Roster, and Data via shell links", async () => {
    renderApp("/");

    fireEvent.click(screen.getByRole("link", { name: "Loops workspace" }));
    await waitFor(() => {
      expect(screen.getByText("LoopsWorkspacePageMarker")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("link", { name: "Results Library" }));
    await waitFor(() => {
      expect(screen.getByText("ResultsLibraryPageMarker")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("link", { name: "Roster & Profile" }));
    await waitFor(() => {
      expect(screen.getByText("RosterProfilePageMarker")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("link", { name: "Data & Mechanics" }));
    await waitFor(() => {
      expect(screen.getByText("DataMechanicsPageMarker")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("link", { name: "Learn" }));
    await waitFor(() => {
      expect(screen.getByText("LearnPageMarker")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("link", { name: "Workspace" }));
    await waitFor(() => {
      expect(screen.getByText("WorkspacePageMarker")).toBeTruthy();
    });
  });
});
