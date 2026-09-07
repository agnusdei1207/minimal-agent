import { createHash } from "node:crypto";
import { chmod, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

import { readResponseBounded } from "./bounded-download.mjs";

if (process.env.PENTESTING_SKIP_DOWNLOAD === "1" || process.env.MINIMAL_AGENT_SKIP_DOWNLOAD === "1") {
  process.exit(0);
}

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));
const executableName = process.platform === "win32" ? "pentesting.exe" : "pentesting";
const asset = `pentesting-${process.platform}-${process.arch}${process.platform === "win32" ? ".exe" : ""}`;
const releaseRoot = `https://github.com/agnusdei1207/pentesting/releases/download/v${manifest.version}`;
const targetDirectory = join(packageRoot, "vendor", `${process.platform}-${process.arch}`);
const target = join(targetDirectory, executableName);
const temporary = `${target}.download`;
const assetMaxBytes = 128 * 1024 * 1024;
const checksumsMaxBytes = 1024 * 1024;
const downloadSignal = AbortSignal.timeout(120_000);

await mkdir(targetDirectory, { recursive: true });
try {
  const [assetResponse, checksumsResponse] = await Promise.all([
    fetch(`${releaseRoot}/${asset}`, { signal: downloadSignal }),
    fetch(`${releaseRoot}/checksums.txt`, { signal: downloadSignal }),
  ]);
  if (!assetResponse.ok) {
    throw new Error(`download ${asset}: HTTP ${assetResponse.status}`);
  }
  if (!checksumsResponse.ok) {
    throw new Error(`download checksums.txt: HTTP ${checksumsResponse.status}`);
  }
  const [bytes, checksumsBytes] = await Promise.all([
    readResponseBounded(assetResponse, assetMaxBytes, asset),
    readResponseBounded(checksumsResponse, checksumsMaxBytes, "checksums.txt"),
  ]);
  const checksums = checksumsBytes.toString("utf8");
  const expected = checksums
    .split(/\r?\n/)
    .map((line) => line.trim().split(/\s+/))
    .find((parts) => parts.length >= 2 && parts.at(-1)?.replace(/^\*/, "") === asset)?.[0];
  if (!expected || !/^[a-f0-9]{64}$/i.test(expected)) {
    throw new Error(`checksums.txt has no SHA-256 entry for ${asset}`);
  }
  const actual = createHash("sha256").update(bytes).digest("hex");
  if (actual.toLowerCase() !== expected.toLowerCase()) {
    throw new Error(`SHA-256 mismatch for ${asset}`);
  }
  await writeFile(temporary, bytes, { mode: 0o755 });
  await chmod(temporary, 0o755);
  await rm(target, { force: true });
  await rename(temporary, target);
} catch (error) {
  await rm(temporary, { force: true });
  throw error;
}
