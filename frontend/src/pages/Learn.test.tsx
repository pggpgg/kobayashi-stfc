import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import Learn from "./Learn";

describe("Learn", () => {
  it("renders the page heading and pipeline diagram", () => {
    render(<Learn />);
    expect(
      screen.getByRole("heading", { name: "Learn", level: 1 }),
    ).toBeTruthy();
    expect(
      screen.getByRole("heading", { name: "How a search runs" }),
    ).toBeTruthy();
    expect(screen.getByRole("img", { name: /candidate funnel/i })).toBeTruthy();
  });

  it("lists implemented methods with In Kobayashi badges", () => {
    render(<Learn />);
    for (const title of [
      "Tiered scout → confirm",
      "Genetic algorithm",
      "Stratified random baseline",
      "Heuristics seeds & warm start",
    ]) {
      expect(screen.getByRole("heading", { name: title })).toBeTruthy();
    }
    expect(screen.getAllByText("In Kobayashi").length).toBeGreaterThanOrEqual(
      8,
    );
  });

  it("lists roadmap methods with Roadmap badges", () => {
    render(<Learn />);
    for (const title of [
      "Local refinement & large-neighborhood repair",
      "Pareto frontier recommendations",
      "Beam search with diversity lanes",
      "Meta-optimizer",
    ]) {
      expect(screen.getByRole("heading", { name: title })).toBeTruthy();
    }
    expect(screen.getAllByText("Roadmap").length).toBeGreaterThanOrEqual(8);
  });

  it("shows strategy chips for API-selectable methods", () => {
    render(<Learn />);
    for (const chip of [
      'strategy: "tiered"',
      'strategy: "genetic"',
      'strategy: "random_stratified"',
      'strategy: "linear_eval"',
    ]) {
      expect(screen.getByText(chip)).toBeTruthy();
    }
  });

  it("links out to the repository docs", () => {
    render(<Learn />);
    const links = screen.getAllByRole("link", {
      name: /read more in the docs/i,
    });
    expect(links.length).toBeGreaterThanOrEqual(10);
    for (const link of links) {
      expect(link.getAttribute("href")).toMatch(
        /^https:\/\/github\.com\/pggpgg\/kobayashi-stfc\/blob\/main\/docs\//,
      );
    }
  });
});
