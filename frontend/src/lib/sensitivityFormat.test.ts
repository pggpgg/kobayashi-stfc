import { describe, expect, it } from "vitest";
import { fmtFloat, fmtPct } from "./sensitivityFormat";

describe("fmtFloat", () => {
  it("formats a normal float to the requested digits", () => {
    expect(fmtFloat(0.123456)).toBe("0.1235");
    expect(fmtFloat(0.123456, 2)).toBe("0.12");
    expect(fmtFloat(-1.5, 1)).toBe("-1.5");
  });

  it("falls back to an em dash for non-finite input", () => {
    expect(fmtFloat(Number.NaN)).toBe("—");
    expect(fmtFloat(Number.POSITIVE_INFINITY)).toBe("—");
    expect(fmtFloat(Number.NEGATIVE_INFINITY)).toBe("—");
  });

  it("switches to exponential notation for tiny non-zero magnitudes", () => {
    expect(fmtFloat(1e-8)).toBe("1.00e-8");
    expect(fmtFloat(-2.5e-9)).toBe("-2.50e-9");
  });

  it("keeps exact zero in fixed notation rather than exponential", () => {
    expect(fmtFloat(0)).toBe("0.0000");
  });
});

describe("fmtPct", () => {
  it("formats a fraction as a percent string", () => {
    expect(fmtPct(0.0512)).toBe("5.12%");
    expect(fmtPct(1, 0)).toBe("100%");
  });

  it("falls back to an em dash for null, undefined, or non-finite input", () => {
    expect(fmtPct(null)).toBe("—");
    expect(fmtPct(undefined)).toBe("—");
    expect(fmtPct(Number.NaN)).toBe("—");
  });
});
