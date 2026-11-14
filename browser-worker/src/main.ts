// Start the browser worker: `npm start` in browser-worker/.
//
//   WORKER_PORT   default 8090 (the API's BROWSER_WORKER_URL points here)
//   PROFILE_DIR   default ~/.careerpilot/browser-profile (keeps your SEEK login)
//   HEADLESS=1    hide the window (only for testing)

import { homedir } from "node:os";
import { join } from "node:path";

import { startServer } from "./server.ts";

const port = Number(process.env.WORKER_PORT ?? 8090);
const profileDir = process.env.PROFILE_DIR ?? join(homedir(), ".careerpilot", "browser-profile");
const headless = process.env.HEADLESS === "1";

const worker = await startServer(port, { profileDir, headless });
console.log(`browser worker listening on ${worker.url} (profile ${profileDir}${headless ? ", headless" : ""})`);
console.log(`practice form: ${worker.url}/practice/quick-apply`);

const stop = async () => {
  console.log("shutting down");
  await worker.close();
  process.exit(0);
};
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
