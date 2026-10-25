import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import * as api from "../lib/api";
import * as sensitivityApi from "../lib/sensitivityApi";
import Sensitivity from "./Sensitivity";

vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof import("../lib/api")>()),
  fetchProfiles: vi.fn(),
  fetchShips: vi.fn(),
  fetchHostiles: vi.fn(),
}));
vi.mock("../lib/sensitivityApi", async (original) => ({
  ...(await original<typeof import("../lib/sensitivityApi")>()),
  fetchSensitivityDefaults: vi.fn(),
  fetchMorrisDefaults: vi.fn(),
  fetchSobolDefaults: vi.fn(),
  sensitivityStart: vi.fn(),
  cancelSensitivityJob: vi.fn(),
}));
class TestEventSource {
  static instances: TestEventSource[] = [];
  onmessage: ((event: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  close = vi.fn();
  constructor(public url: string) {
    TestEventSource.instances.push(this);
  }
}
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  TestEventSource.instances = [];
  vi.stubGlobal("EventSource", TestEventSource);
  HTMLElement.prototype.scrollIntoView = vi.fn();
  vi.mocked(api.fetchProfiles).mockResolvedValue({
    profiles: [{ id: "p1", name: "Main", sync_token: "t" }],
    default_id: "p1",
  });
  vi.mocked(api.fetchShips).mockResolvedValue([
    { id: "s1", ship_name: "Realta", ship_class: "explorer" },
  ]);
  vi.mocked(api.fetchHostiles).mockResolvedValue([
    { id: "h1", hostile_name: "Bandit", level: 26, ship_class: "explorer" },
  ]);
  vi.mocked(sensitivityApi.fetchSensitivityDefaults).mockResolvedValue({
    deltas: [],
  });
  vi.mocked(sensitivityApi.fetchMorrisDefaults).mockResolvedValue({
    deltas: [],
    num_sims_default: 200,
    num_sims_max: 1000,
    r_trajectories_default: 10,
    r_trajectories_max: 100,
  });
  vi.mocked(sensitivityApi.fetchSobolDefaults).mockResolvedValue({
    deltas: [],
    n_samples_default: 512,
    n_samples_max: 1024,
  });
  vi.mocked(sensitivityApi.cancelSensitivityJob).mockResolvedValue(undefined);
  vi.mocked(sensitivityApi.sensitivityStart).mockResolvedValue({
    job_id: "sensitivity-job",
  });
});
afterEach(() => {
  vi.unstubAllGlobals();
  Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
});

async function beginRun() {
  const view = render(
    <ProfileProvider>
      <Sensitivity />
    </ProfileProvider>,
  );
  await screen.findByRole("option", { name: "Realta" });
  fireEvent.change(screen.getByRole("combobox", { name: "Ship" }), {
    target: { value: "s1" },
  });
  fireEvent.focus(screen.getByRole("combobox", { name: "Scenario" }));
  fireEvent.mouseDown(await screen.findByRole("option", { name: /Bandit/ }));
  fireEvent.click(
    screen.getByRole("button", { name: "Run sensitivity analysis" }),
  );
  await waitFor(() =>
    expect(sensitivityApi.sensitivityStart).toHaveBeenCalledTimes(1),
  );
  return view;
}

describe("#343 sensitivity job ownership", () => {
  it("cancels the server job as well as closing its stream on unmount", async () => {
    const view = await beginRun();
    await waitFor(() => expect(TestEventSource.instances).toHaveLength(1));
    view.unmount();
    expect(TestEventSource.instances[0].close).toHaveBeenCalled();
    expect(sensitivityApi.cancelSensitivityJob).toHaveBeenCalledWith(
      "sensitivity-job",
    );
  });
  it("cancels a job accepted after the page was unmounted without opening a new stream", async () => {
    let finish!: (response: { job_id: string }) => void;
    vi.mocked(sensitivityApi.sensitivityStart).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const view = await beginRun();
    view.unmount();
    await act(async () => finish({ job_id: "late-job" }));
    expect(sensitivityApi.cancelSensitivityJob).toHaveBeenCalledWith(
      "late-job",
    );
    expect(TestEventSource.instances).toHaveLength(0);
  });
  it("does not cancel a completed job on unmount", async () => {
    const view = await beginRun();
    await waitFor(() => expect(TestEventSource.instances).toHaveLength(1));
    act(() =>
      TestEventSource.instances[0].onmessage?.({
        data: JSON.stringify({
          status: "error",
          error: "Finished with an error",
        }),
      }),
    );
    view.unmount();
    expect(sensitivityApi.cancelSensitivityJob).not.toHaveBeenCalled();
  });
});
