// Vitest setup for jsdom environment.
// This file runs before each test file.

import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Automatically unmount React trees after each test.
afterEach(() => {
  cleanup();
});
