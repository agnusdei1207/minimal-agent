import { readFile } from "node:fs/promises";

const expectedFiles = [
  "LICENSE",
  "README.md",
  "bin/minimal-agent.js",
  "package.json",
  "scripts/bounded-download.mjs",
  "scripts/install.mjs",
];

const reportPath = process.argv[2];
if (!reportPath) {
  throw new Error("usage: node scripts/verify-pack.mjs <npm-pack-report.json>");
}

const report = JSON.parse(await readFile(reportPath, "utf8"));
const manifest = JSON.parse(
  await readFile(new URL("../package.json", import.meta.url), "utf8"),
);
if (!Array.isArray(report) || report.length !== 1) {
  throw new Error("npm pack must describe exactly one package");
}

const packageReport = report[0];
if (packageReport.name !== manifest.name || packageReport.version !== manifest.version) {
  throw new Error("npm pack identity does not match package.json");
}

const actualFiles = (packageReport.files ?? []).map(({ path }) => path).sort();
const expected = [...expectedFiles].sort();
if (JSON.stringify(actualFiles) !== JSON.stringify(expected)) {
  throw new Error(
    `npm pack boundary mismatch\nexpected: ${expected.join(", ")}\nactual: ${actualFiles.join(", ")}`,
  );
}
