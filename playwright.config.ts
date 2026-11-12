import { existsSync } from "node:fs";
import { defineConfig } from "@playwright/test";

// The tests drive the built app in a real browser, with the real engine.
// `pnpm build` first; `pnpm test:ui` then serves dist/ and runs them.
const chromium = process.env.MJOLNIR_CHROMIUM ?? "/opt/pw-browsers/chromium";

export default defineConfig({
  testDir: "tests",
  timeout: 60_000,
  workers: 2,
  reporter: "list",
  use: {
    baseURL: "http://localhost:4174",
    launchOptions: {
      args: ["--autoplay-policy=no-user-gesture-required"],
      ...(existsSync(chromium) ? { executablePath: chromium } : {}),
    },
  },
  projects: [
    { name: "phone", use: { viewport: { width: 412, height: 800 }, hasTouch: true, isMobile: true } },
    { name: "desk", use: { viewport: { width: 1280, height: 800 } } },
  ],
  webServer: {
    command: "pnpm exec vite preview --port 4174 --strictPort",
    url: "http://localhost:4174",
    reuseExistingServer: true,
  },
});
