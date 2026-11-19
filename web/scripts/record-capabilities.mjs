// The simulated devices' capabilities, recorded for the unit tests of the
// panes built from them, from a server of the script's own
// (tests/e2e/server.ts): the camera's (W3.2) and the lights' (W3.4). The
// end-to-end tests check that the server still says the same.
//
//   node scripts/record-capabilities.mjs    (after `cargo build -p oscope-server`)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

/** Each Thing's recording, by where it is kept. */
const RECORDED = {
  camera: "../src/features/control/camera/recorded-capabilities.json",
  illumination: "../src/features/control/lights/recorded-capabilities.json",
};

const server = new Server();
await server.start();
try {
  for (const [thing, path] of Object.entries(RECORDED)) {
    const answer = await fetch(`${server.url}/api/v1/${thing}/capabilities`);
    if (!answer.ok) throw new Error(`${thing}/capabilities: ${answer.status}`);
    const out = fileURLToPath(new URL(path, import.meta.url));
    writeFileSync(out, `${JSON.stringify(await answer.json(), null, 2)}\n`);
    console.log(`record-capabilities: wrote ${out}`);
  }
} finally {
  await server.dispose();
}
