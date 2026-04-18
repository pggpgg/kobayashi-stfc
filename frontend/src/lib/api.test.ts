import { afterEach, describe, expect, it, vi } from "vitest";
import {
  API_ERROR_CPU_BUSY,
  ApiError,
  formatApiError,
  parseApiError,
  simulate,
} from "./api";

describe("ApiError", () => {
  it("has correct name and fields", () => {
    const err = new ApiError("not found", 404, "NOT_FOUND");
    expect(err.name).toBe("ApiError");
    expect(err.message).toBe("not found");
    expect(err.status).toBe(404);
    expect(err.code).toBe("NOT_FOUND");
    expect(err.retryAfterMs).toBeUndefined();
    expect(err).toBeInstanceOf(Error);
  });

  it("stores optional retryAfterMs", () => {
    const err = new ApiError("busy", 503, API_ERROR_CPU_BUSY, 250);
    expect(err.retryAfterMs).toBe(250);
  });
});

describe("parseApiError", () => {
  it("extracts message from JSON body", async () => {
    const res = {
      status: 400,
      statusText: "Bad Request",
      ok: false,
    } as Response;
    const body = JSON.stringify({ status: "error", message: "Ship not found" });
    const err = await parseApiError(res, body);
    expect(err.message).toBe("Ship not found");
    expect(err.status).toBe(400);
    expect(err.code).toBe("VALIDATION");
  });

  it("falls back to body text when JSON has no message", async () => {
    const res = {
      status: 500,
      statusText: "Internal Server Error",
      ok: false,
    } as Response;
    const body = "plain error text";
    const err = await parseApiError(res, body);
    expect(err.message).toBe("plain error text");
    expect(err.code).toBe("SERVER_ERROR");
  });

  it("falls back to statusText when body is empty", async () => {
    const res = { status: 404, statusText: "Not Found", ok: false } as Response;
    const err = await parseApiError(res, "");
    expect(err.message).toBe("Not Found");
    expect(err.code).toBe("NOT_FOUND");
  });

  it("maps 401 to AUTH code", async () => {
    const res = {
      status: 401,
      statusText: "Unauthorized",
      ok: false,
    } as Response;
    const err = await parseApiError(res, "");
    expect(err.code).toBe("AUTH");
  });

  it("maps cpu_busy JSON to CPU_BUSY with retry_after_ms", async () => {
    const body = JSON.stringify({
      status: "error",
      code: "cpu_busy",
      message: "Server CPU capacity is saturated; retry later.",
      retry_after_ms: 500,
    });
    const res = new Response(body, {
      status: 503,
      statusText: "Service Unavailable",
      headers: { "Content-Type": "application/json" },
    });
    const err = await parseApiError(res, body);
    expect(err.code).toBe(API_ERROR_CPU_BUSY);
    expect(err.status).toBe(503);
    expect(err.retryAfterMs).toBe(500);
    expect(err.message).toContain("saturated");
  });

  it("uses Retry-After header when retry_after_ms missing", async () => {
    const body = JSON.stringify({
      status: "error",
      code: "cpu_busy",
      message: "Busy",
    });
    const res = new Response(body, {
      status: 503,
      statusText: "Service Unavailable",
      headers: {
        "Content-Type": "application/json",
        "Retry-After": "3",
      },
    });
    const err = await parseApiError(res, body);
    expect(err.code).toBe(API_ERROR_CPU_BUSY);
    expect(err.retryAfterMs).toBe(3000);
  });
});

describe("formatApiError", () => {
  it("adds retry hint for server errors", () => {
    const err = new ApiError("Internal error", 500, "SERVER_ERROR");
    expect(formatApiError(err)).toBe("Internal error Try again later.");
  });

  it("returns plain message for non-server errors", () => {
    const err = new ApiError("Bad input", 400, "VALIDATION");
    expect(formatApiError(err)).toBe("Bad input");
  });

  it("handles non-Error values", () => {
    expect(formatApiError("some string")).toBe("some string");
    expect(formatApiError(42)).toBe("42");
  });

  it("handles generic Error", () => {
    expect(formatApiError(new Error("generic"))).toBe("generic");
  });

  it("formats CPU_BUSY with retry hint", () => {
    const err = new ApiError(
      "Server CPU capacity is saturated; retry later.",
      503,
      API_ERROR_CPU_BUSY,
      2500,
    );
    const out = formatApiError(err);
    expect(out).toContain("saturated");
    expect(out).toMatch(/3s|about 3/);
  });

  it("formats CPU_BUSY without retry as generic busy guidance", () => {
    const err = new ApiError("Server busy.", 503, API_ERROR_CPU_BUSY);
    const out = formatApiError(err);
    expect(out).toContain("another simulation or optimization");
  });
});

describe("simulate cpu_busy retry", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("waits once and retries after cpu_busy", async () => {
    vi.useFakeTimers();
    const stats = {
      win_rate: 0.5,
      stall_rate: 0,
      loss_rate: 0.5,
      avg_hull_remaining: 0.5,
      avg_defender_hull_remaining: 0.5,
      n: 10,
    };
    const okBody = JSON.stringify({
      status: "ok",
      stats,
      seed: 42,
    });
    const busyBody = JSON.stringify({
      status: "error",
      code: "cpu_busy",
      message: "busy",
      retry_after_ms: 100,
    });
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(busyBody, {
          status: 503,
          headers: { "Content-Type": "application/json" },
        }),
      )
      .mockResolvedValueOnce(
        new Response(okBody, {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }),
      );
    vi.stubGlobal("fetch", fetchMock);

    const crew = {
      captain: "Kirk",
      bridge: ["Spock", null] as (string | null)[],
      below_deck: [] as (string | null)[],
    };
    const p = simulate({
      ship: "saladin",
      hostile: "2918121098",
      crew,
      num_sims: 10,
    });
    await vi.advanceTimersByTimeAsync(100);
    const result = await p;
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(result.stats.n).toBe(10);
    expect(result.seed).toBe(42);
  });
});
