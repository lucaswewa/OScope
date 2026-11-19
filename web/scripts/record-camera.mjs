// The simulated camera's capabilities, recorded for the camera fields' unit
// tests (src/features/control/camera/cameraFields.test.ts, W3.2), from a
// server of the script's own (tests/e2e/server.ts). The end-to-end tests
// check that the server still says the same.
//
//   node scripts/record-camera.mjs    (after `cargo build -p oscope-server`)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(
  new URL("../src/features/control/camera/recorded-capabilities.json", import.meta.url),
);
const server = new Server();
await server.start();
try {
  const answer = await fetch(`${server.url}/api/v1/camera/capabilities`);
  if (!answer.ok) throw new Error(`camera/capabilities: ${answer.status}`);
  writeFileSync(OUT, `${JSON.stringify(await answer.json(), null, 2)}\n`);
  console.log(`record-camera: wrote ${OUT}`);
} finally {
  await server.dispose();
}
