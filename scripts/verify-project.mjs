import { readFile } from "node:fs/promises";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");
const manifest = JSON.parse(await read("package.json"));
const lock = JSON.parse(await read("package-lock.json"));
const version = manifest.version;

function requireEqual(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(`${label}: expected ${expected}, got ${actual}`);
  }
}

async function requireText(path, fragments) {
  const content = await read(path);
  for (const fragment of fragments) {
    if (!content.includes(fragment)) {
      throw new Error(`${path} is missing ${JSON.stringify(fragment)}`);
    }
  }
}

function hasInstruction(dockerfileContent, instruction) {
  return dockerfileContent
    .split(/\r?\n/)
    .some((line) => !line.trim().startsWith("#") && line.includes(instruction));
}

requireEqual(manifest.name, "pentesting", "package name");
requireEqual(manifest.engines?.node, ">=24 <25", "Node engine");
if (!manifest.description?.includes("powered by Rust")) {
  throw new Error("package description must contain 'powered by Rust'");
}
requireEqual(lock.name, manifest.name, "package-lock name");
requireEqual(lock.version, version, "package-lock version");
requireEqual(lock.packages?.[""]?.version, version, "package-lock root version");
requireEqual(manifest.scripts?.check, "npm run check:docker", "npm check route");

requireEqual(
  manifest.scripts?.["check:docker"],
  "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check.ps1",
  "docker check route",
);
await requireText("scripts/check.ps1", [
  "dimage.ps1",
  "-Target all",
  "'run', '-it', '--rm'",
  "'--memory', '2g'",
  "'--memory-swap', '2g'",
  "'--cpus', '2'",
  "'--pids-limit', '512'",
  "pentesting-workspace:/workspace",
  "pentesting-state:/state",
  "OPENAI_API_KEY",
  "OPENAI_MODEL",
  "OPENROUTER_API_KEY",
  "pentesting:check",
  "--run",
  "/state/check-",
]);
if (!manifest.scripts?.["docker:build"]?.includes("scripts/dimage.ps1")) {
  throw new Error("docker:build must use the capped image wrapper");
}
if (!manifest.scripts?.["docker:base"]?.includes("scripts/dimage.ps1")) {
  throw new Error("docker:base must use the capped image wrapper");
}

await requireText("Cargo.toml", [
  `name = "${manifest.name}"`,
  `version = "${version}"`,
  'rust-version = "1.98"',
  'repository = "https://github.com/agnusdei1207/pentesting"',
]);
await requireText("rust-toolchain.toml", ['channel = "1.98.0"']);
requireEqual((await read(".nvmrc")).trim(), "24", ".nvmrc");
await requireText("docker/app.Dockerfile", [
  "FROM rust:1.98-bookworm AS builder",
  "FROM runtime-base",
  `ARG VERSION=${version}`,
  "USER 10001:10001",
]);
if (hasInstruction(await read("docker/app.Dockerfile"), "apt-get")) {
  throw new Error("docker/app.Dockerfile must not add an apt-get layer");
}
await requireText("docker/runtime-base.Dockerfile", [
  "FROM ubuntu:26.04",
  "ARG CHROME_FOR_TESTING_VERSION=",
  "apt update",
  "apt install -y --no-install-recommends",
  "docker/install-browser.sh",
]);
if (hasInstruction(await read("docker/runtime-base.Dockerfile"), "apt-get")) {
  throw new Error("docker/runtime-base.Dockerfile must use apt, not apt-get");
}
await requireText("docker/install-browser.sh", [
  "CHROME_FOR_TESTING_VERSION",
  "CHROME_FOR_TESTING_SHA256",
  "sha256sum --check",
]);
const compose = await read("docker/compose.yaml");
if (!compose.includes(`image: agnusdei1207/pentesting:${version}`)) {
  throw new Error("docker/compose.yaml must use the versioned prebuilt image");
}
if (compose.includes("build:") || compose.includes("dockerfile:")) {
  throw new Error("docker/compose.yaml must not bypass scripts/dimage.ps1");
}
await requireText("README.md", ["npm install --global pentesting"]);
await requireText("docs/adr/ADR-0006-pentesting-successor-and-unified-release-pipeline.md", [
  `Version target: ${version}`,
]);
