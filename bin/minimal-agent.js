#!/usr/bin/env node

import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const executableName = process.platform === "win32" ? "minimal-agent.exe" : "minimal-agent";
const platformBinary = join(
  packageRoot,
  "vendor",
  `${process.platform}-${process.arch}`,
  executableName,
);
const binary = process.env.MINIMAL_AGENT_BINARY
  ? resolve(process.env.MINIMAL_AGENT_BINARY)
  : platformBinary;

if (!existsSync(binary)) {
  console.error(
    `minimal-agent native binary is missing at ${binary}. Reinstall the package or set MINIMAL_AGENT_BINARY.`,
  );
  process.exit(1);
}

const isJavaScript = binary.endsWith(".js") || binary.endsWith(".mjs") || binary.endsWith(".cjs");
const command = isJavaScript ? process.execPath : binary;
const args = isJavaScript ? [binary, ...process.argv.slice(2)] : process.argv.slice(2);
const result = spawnSync(command, args, { stdio: "inherit", windowsHide: true });
if (result.error) {
  console.error(`failed to launch minimal-agent: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
