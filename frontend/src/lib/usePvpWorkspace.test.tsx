import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import { WorkspaceModeProvider } from "../contexts/WorkspaceModeContext";
import { usePvpWorkspace } from "./usePvpWorkspace";

vi.mock("./api", () => ({
  fetchProfiles: vi.fn().mockResolvedValue({
    profiles: [{ id: "demo", name: "Demo", sync_token: "tok" }],
    default_id: "demo",
  }),
  formatApiError: vi.fn((e: unknown) =>
    e instanceof Error ? e.message : String(e),
  ),
  simulate: vi.fn(),
  optimizeStart: vi.fn(),
  getOptimizeStatus: vi.fn(),
}));

function wrapper({ children }: { children: ReactNode }) {
  return (
    <WorkspaceModeProvider>
      <ProfileProvider>{children}</ProfileProvider>
    </WorkspaceModeProvider>
  );
}

describe("usePvpWorkspace opponent profile", () => {
  it("auto-selects the only profile when it is also active", async () => {
    const { result } = renderHook(() => usePvpWorkspace(), { wrapper });

    await waitFor(() => {
      expect(result.current.opponentProfileId).toBe("demo");
    });
  });
});
