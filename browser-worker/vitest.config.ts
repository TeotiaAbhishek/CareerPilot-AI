import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    // Each test file starts a real (headless) Chromium.
    testTimeout: 30_000,
    hookTimeout: 60_000,
  },
});
