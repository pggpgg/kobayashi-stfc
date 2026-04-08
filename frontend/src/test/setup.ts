// Vitest setup for jsdom environment.
// This file runs before each test file.

import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Node 25+ may inject experimental `localStorage` (e.g. with `--localstorage-file`)
// that lacks `Storage.prototype.clear`, which breaks tests that call `localStorage.clear()`.
function memoryStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() {
      return map.size;
    },
    clear() {
      map.clear();
    },
    getItem(key: string) {
      return map.get(String(key)) ?? null;
    },
    setItem(key: string, value: string) {
      map.set(String(key), String(value));
    },
    removeItem(key: string) {
      map.delete(String(key));
    },
    key(index: number) {
      return [...map.keys()][index] ?? null;
    },
  } as Storage;
}

const g = globalThis as typeof globalThis & { localStorage: Storage };
if (typeof g.localStorage?.clear !== "function") {
  g.localStorage = memoryStorage();
}

// Automatically unmount React trees after each test.
afterEach(() => {
  cleanup();
});
