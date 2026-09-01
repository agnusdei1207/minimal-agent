// XBOW-104 artifact management — atomic writes, SHA-256 audit manifests,
// stream teeing, and command evidence capture.

import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

/** SHA-256 hex digest of a file. */
export function sha256File(file) {
  return createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

/** Copy source → destination only when SHA-256 differs. */
export function syncFileBySha256(source, destination) {
  if (!fs.existsSync(source))
    throw new Error(`source artifact missing: ${source}`);
  const srcHash = sha256File(source);
  const dstHash = fs.existsSync(destination) ? sha256File(destination) : null;
  const changed = srcHash !== dstHash;
  if (changed) {
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.copyFileSync(source, destination);
  }
  return { changed, sha256: srcHash };
}

/** Write JSON atomically via tmp-rename. */
export function writeJsonAtomic(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const tmp = `${file}.${process.pid}.tmp`;
  fs.writeFileSync(tmp, `${JSON.stringify(value, null, 2)}\n`);
  try {
    fs.renameSync(tmp, file);
  } catch {
    if (fs.existsSync(file)) fs.rmSync(file);
    fs.renameSync(tmp, file);
  }
}

/** Persist run-state.json with an updated_at timestamp. */
export function writeRunState(runDir, state) {
  const value = { ...state, updated_at: new Date().toISOString() };
  writeJsonAtomic(path.join(runDir, "run-state.json"), value);
  return value;
}

/** Write stdout/stderr/result.json for a command into harness/. */
export function writeCommandArtifacts(runDir, label, result) {
  const dir = path.join(runDir, "harness");
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, `${label}.stdout.log`),
    String(result.stdout || ""),
  );
  fs.writeFileSync(
    path.join(dir, `${label}.stderr.log`),
    String(result.stderr || ""),
  );
  writeJsonAtomic(path.join(dir, `${label}.result.json`), {
    ok: result.ok === true,
    code: result.code ?? (result.ok ? 0 : null),
  });
}

/** Tee a readable stream into a file stream and a visible stream. */
export function teeStream(source, transcript, visible = process.stdout) {
  source.on("data", (chunk) => {
    transcript.write(chunk);
    visible.write(chunk);
  });
}

function readTail(file, maxBytes = 256 * 1024) {
  if (!fs.existsSync(file)) return "";
  const size = fs.statSync(file).size;
  const len = Math.min(size, maxBytes);
  const buf = Buffer.alloc(len);
  const fd = fs.openSync(file, "r");
  try {
    fs.readSync(fd, buf, 0, len, size - len);
  } finally {
    fs.closeSync(fd);
  }
  return buf.toString("utf8");
}

/**
 * Spawn a command and capture stdout/stderr to harness/ files.
 * Returns { ok, code, stdout, stderr }.
 */
export function runCommandWithArtifacts(
  runDir,
  label,
  command,
  args,
  options = {},
) {
  const dir = path.join(runDir, "harness");
  fs.mkdirSync(dir, { recursive: true });
  const outFile = path.join(dir, `${label}.stdout.log`);
  const errFile = path.join(dir, `${label}.stderr.log`);
  const outFd = fs.openSync(outFile, "w");
  const errFd = fs.openSync(errFile, "w");
  return new Promise((resolve) => {
    let settled = false;
    const child = spawn(command, args, {
      ...options,
      stdio: ["ignore", outFd, errFd],
    });
    const finish = (code, error = null) => {
      if (settled) return;
      settled = true;
      fs.closeSync(outFd);
      fs.closeSync(errFd);
      const result = {
        ok: code === 0 && !error,
        code: code ?? -1,
        stdout: readTail(outFile),
        stderr: error
          ? `${readTail(errFile)}\n${error.message}`.trim()
          : readTail(errFile),
      };
      writeJsonAtomic(path.join(dir, `${label}.result.json`), {
        ok: result.ok,
        code: result.code,
      });
      resolve(result);
    };
    child.on("exit", (code) => finish(code));
    child.on("error", (err) => finish(-1, err));
  });
}

/** Recursively list files under root (skips sockets and the manifest itself). */
function listFiles(root, current = root) {
  if (!fs.existsSync(current)) return [];
  return fs
    .readdirSync(current, { withFileTypes: true })
    .flatMap((entry) => {
      if (entry.isSocket?.() || entry.name.endsWith(".sock")) return [];
      const abs = path.join(current, entry.name);
      if (entry.isDirectory()) return listFiles(root, abs);
      const rel = path.relative(root, abs).split(path.sep).join("/");
      return rel === "audit-manifest.json" || rel.endsWith(".tmp") ? [] : [rel];
    });
}

/**
 * Write audit-manifest.json — SHA-256 + byte size of every evidence file
 * in a run directory, enabling tamper detection after the fact.
 */
export function writeAuditManifest(runDir, { complete }) {
  const files = listFiles(runDir)
    .sort()
    .flatMap((rel) => {
      const abs = path.join(runDir, ...rel.split("/"));
      try {
        return [
          { path: rel, bytes: fs.statSync(abs).size, sha256: sha256File(abs) },
        ];
      } catch (err) {
        if (["EACCES", "EPERM", "ENOENT"].includes(err?.code)) return [];
        throw err;
      }
    });
  const manifest = {
    schema_version: 1,
    generated_at: new Date().toISOString(),
    complete: complete === true,
    files,
  };
  writeJsonAtomic(path.join(runDir, "audit-manifest.json"), manifest);
  return manifest;
}

/** Verify an existing audit manifest — returns { status, checked, mismatches }. */
export function verifyAuditManifest(runDir) {
  const file = path.join(runDir, "audit-manifest.json");
  if (!fs.existsSync(file))
    return { status: "missing", checked: 0, mismatches: [] };
  try {
    const manifest = JSON.parse(fs.readFileSync(file, "utf8"));
    const mismatches = [];
    for (const entry of Array.isArray(manifest.files) ? manifest.files : []) {
      const abs = path.join(runDir, ...String(entry.path).split("/"));
      if (!fs.existsSync(abs)) mismatches.push(`${entry.path}:missing`);
      else if (fs.statSync(abs).size !== entry.bytes)
        mismatches.push(`${entry.path}:size`);
      else if (sha256File(abs) !== entry.sha256)
        mismatches.push(`${entry.path}:sha256`);
    }
    return {
      status: mismatches.length ? "mismatch" : "ok",
      complete: manifest.complete === true,
      checked: manifest.files?.length || 0,
      mismatches,
    };
  } catch (err) {
    return { status: "invalid", checked: 0, mismatches: [String(err.message)] };
  }
}
