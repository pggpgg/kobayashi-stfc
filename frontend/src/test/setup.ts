// Vitest setup for jsdom environment.
// This file runs before each test file.
import { afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';

// Automatically unmount React trees after each test.
afterEach(() => {
  cleanup();
});
