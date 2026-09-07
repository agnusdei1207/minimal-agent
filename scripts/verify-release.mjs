import { readFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
const version = manifest.version;

console.log(`[verify-release] Verifying release readiness for ${manifest.name}@${version}...`);

if (manifest.name !== "pentesting") {
  throw new Error(`expected package name "pentesting", got "${manifest.name}"`);
}

// 1. Run verify-project.mjs
console.log("[verify-release] Checking project integrity...");
execFileSync("node", ["scripts/verify-project.mjs"], { cwd: root, stdio: "inherit" });

// 2. Run npm pack --dry-run and verify boundary
console.log("[verify-release] Checking npm pack manifest boundary...");
const packRaw = execFileSync("npm", ["pack", "--dry-run", "--json"], { cwd: root, encoding: "utf8" });
const tempReport = join(root, ".npm-pack-temp.json");
await (await import("node:fs/promises")).writeFile(tempReport, packRaw, "utf8");
try {
  execFileSync("node", ["scripts/verify-pack.mjs", tempReport], { cwd: root, stdio: "inherit" });
} finally {
  await (await import("node:fs/promises")).rm(tempReport, { force: true });
}

console.log(`[verify-release] Release verification SUCCESS for ${manifest.name}@${version}`);
