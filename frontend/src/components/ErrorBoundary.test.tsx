import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import ErrorBoundary from "./ErrorBoundary";

function Boom(): never {
  throw new Error("test boom");
}

describe("ErrorBoundary", () => {
  it("shows fallback when a child throws", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <MemoryRouter>
        <ErrorBoundary>
          <Boom />
        </ErrorBoundary>
      </MemoryRouter>,
    );
    expect(screen.getByRole("alert")).toBeTruthy();
    expect(screen.getByText("test boom")).toBeTruthy();
    expect(screen.getByText("Something went wrong")).toBeTruthy();
    vi.mocked(console.error).mockRestore();
  });
});
