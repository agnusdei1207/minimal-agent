import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import { readResponseBounded } from "../scripts/bounded-download.mjs";

test("launcher forwards arguments and exit status to the selected binary", async () => {
  const directory = await mkdtemp(join(tmpdir(), "minimal-agent-launcher-"));
  const helper = join(directory, "helper.mjs");
  await writeFile(
    helper,
    "console.log(JSON.stringify(process.argv.slice(2))); process.exit(7);\n",
    { mode: 0o755 },
  );
  await chmod(helper, 0o755);
  const result = spawnSync(
    process.execPath,
    [resolve("bin/minimal-agent.js"), "alpha", "two words"],
    {
      cwd: resolve("."),
      env: { ...process.env, MINIMAL_AGENT_BINARY: helper },
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 7);
  assert.equal(result.stdout.trim(), '["alpha","two words"]');
});

test("launcher fails clearly when the native binary is absent", () => {
  const result = spawnSync(process.execPath, [resolve("bin/minimal-agent.js")], {
    cwd: resolve("."),
    env: { ...process.env, MINIMAL_AGENT_BINARY: resolve("missing-binary") },
    encoding: "utf8",
  });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /native binary is missing/);
});

test("release downloads reject declared and streamed bodies above their envelope", async () => {
  await assert.rejects(
    readResponseBounded(
      new Response("small", { headers: { "content-length": "6" } }),
      5,
      "asset",
    ),
    /asset exceeds 5 bytes/,
  );

  const streamed = new Response(
    new ReadableStream({
      start(controller) {
        controller.enqueue(new Uint8Array([1, 2, 3]));
        controller.enqueue(new Uint8Array([4, 5, 6]));
        controller.close();
      },
    }),
  );
  await assert.rejects(readResponseBounded(streamed, 5, "asset"), /asset exceeds 5 bytes/);
});
