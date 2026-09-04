#!/usr/bin/env node
// Patch the (gitignored) xbow-validation-benchmarks suite so challenges that
// went unbuildable over time build again. Idempotent — safe to re-run.
//
// Three time-rot fixes, none of which alter the challenge/flag logic:
//   1. apt-archive : EOL Debian (buster/stretch/bullseye) mirrors 404 on
//      deb.debian.org — they moved to archive.debian.org. Repoint sources +
//      disable Valid-Until so `apt-get update` works again.
//   2. phantomjs   : the `phantomjs` apt package was removed from Debian.
//      Replace the apt install with the upstream 2.1.1 static binary. The
//      challenge Dockerfiles already set OPENSSL_CONF=/dev/null, which is what
//      that binary needs at runtime.
//   3. expose-map  : `expose: - N:M` is invalid (expose takes a container port
//      only); newer compose rejects it with "invalid start port 'N:M'".
//      Rewrite to `expose: - N`.
//   4. composer-pin : recent `composer:latest` refuses to install packages that
//      carry a security advisory ("… not loaded, because they are affected by
//      security advisories …" → `composer install` exits 2). The challenges pin
//      old, deliberately-vulnerable deps (e.g. twig/twig 1.19.0), so pin the
//      composer stage to 2.7 (pre-blocking) to restore the install.
//
// NOT handled here (needs docker-engine repair, not a file edit): old base
// images like mysql:5.7.15 failing with `failed to load cache key: "" failed
// validation` due to a corrupt local content-store layer.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SUITE = path.resolve(
  process.env.XBOW104_SUITE_DIR ||
    path.join(__dirname, "..", "xbow-validation-benchmarks"),
  "benchmarks",
);

const ARCHIVE_MARKER = "# xbow-patch:archive-sources";
// Only rewrite when sources.list names an ARCHIVED Debian release (buster and
// older 404 on deb.debian.org — they moved to archive.debian.org). bullseye /
// bookworm are still live on deb.debian.org + security.debian.org, so leave
// them untouched (repointing them to archive 404s, and dropping security causes
// version skew with the image's pre-installed security packages). For archived
// releases, repoint BOTH main and security to archive (archive carries e.g.
// `buster/updates`), and drop only the `*-updates` suite (not on archive).
const ARCHIVE_RUN =
  `${ARCHIVE_MARKER}\n` +
  `RUN if [ -f /etc/apt/sources.list ] && grep -qE 'buster|stretch|jessie|wheezy' /etc/apt/sources.list; then ` +
  `sed -i -E '/-updates/d; s|https?://deb.debian.org|http://archive.debian.org|g; ` +
  `s|https?://security.debian.org|http://archive.debian.org|g' /etc/apt/sources.list ` +
  `&& echo 'Acquire::Check-Valid-Until "false";' > /etc/apt/apt.conf.d/99no-check; fi`;

const PHANTOM_MARKER = "# xbow-patch:phantomjs-binary";
const PHANTOM_RUN =
  `${PHANTOM_MARKER}\n` +
  `RUN apt-get update && apt-get install -y --no-install-recommends ` +
  `curl bzip2 libfontconfig1 ca-certificates ` +
  `&& curl -fsSL https://bitbucket.org/ariya/phantomjs/downloads/phantomjs-2.1.1-linux-x86_64.tar.bz2 -o /tmp/p.tar.bz2 ` +
  `&& tar -xjf /tmp/p.tar.bz2 -C /tmp ` +
  `&& cp /tmp/phantomjs-2.1.1-linux-x86_64/bin/phantomjs /usr/local/bin/phantomjs ` +
  `&& chmod +x /usr/local/bin/phantomjs ` +
  `&& rm -rf /tmp/p.tar.bz2 /tmp/phantomjs-2.1.1-linux-x86_64`;

// 4. composer-pin: newer `composer:latest` blocks installing advisory-affected
// packages (the challenges pin old vulnerable deps on purpose), so pin the
// composer builder stage to 2.7, which predates that block. Idempotent: once
// rewritten there is no `composer:latest` left to match.
const COMPOSER_PIN = "composer:2.7";

const stats = { dockerfiles: 0, archive: 0, phantomjs: 0, composeExpose: 0, composerPin: 0 };

function listDirs(root) {
  try {
    return fs
      .readdirSync(root, { withFileTypes: true })
      .filter((d) => d.isDirectory())
      .map((d) => path.join(root, d.name));
  } catch {
    return [];
  }
}

function findFiles(dir, predicate, acc = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) findFiles(p, predicate, acc);
    else if (predicate(e.name)) acc.push(p);
  }
  return acc;
}

function patchDockerfile(file) {
  const original = fs.readFileSync(file, "utf8");
  let text = original;
  const usesApt = /apt-get/.test(text);

  // 2. phantomjs: replace any RUN line that apt-installs phantomjs.
  if (/apt-get install[^\n]*phantomjs/.test(text) && !text.includes(PHANTOM_MARKER)) {
    text = text.replace(
      /^RUN[^\n]*apt-get install[^\n]*phantomjs[^\n]*$/m,
      PHANTOM_RUN,
    );
    stats.phantomjs++;
  }

  // 4. composer-pin: pin `composer:latest` → 2.7 (pre-advisory-block).
  if (/composer:latest/.test(text)) {
    text = text.replace(/composer:latest/g, COMPOSER_PIN);
    stats.composerPin++;
  }

  // 1. apt-archive: replace an existing patch block, else inject after FROM.
  if (usesApt) {
    if (text.includes(ARCHIVE_MARKER)) {
      const replaced = text.replace(
        new RegExp(`${ARCHIVE_MARKER}\\nRUN [^\\n]*`),
        ARCHIVE_RUN,
      );
      if (replaced !== text) {
        text = replaced;
        stats.archive++;
      }
    } else {
      text = text.replace(/^(FROM[^\n]*\n)/m, `$1${ARCHIVE_RUN}\n`);
      stats.archive++;
    }
  }

  if (text !== original) {
    fs.writeFileSync(file, text);
    stats.dockerfiles++;
  }
}

function patchCompose(file) {
  const lines = fs.readFileSync(file, "utf8").split(/\r?\n/);
  let changed = false;
  let inExpose = false;
  let exposeIndent = -1;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const m = line.match(/^(\s*)expose:\s*$/);
    if (m) {
      inExpose = true;
      exposeIndent = m[1].length;
      continue;
    }
    if (inExpose) {
      const item = line.match(/^(\s*)-\s*(\d+):(\d+)\s*$/);
      if (item && item[1].length > exposeIndent) {
        // keep the container port only
        lines[i] = `${item[1]}- ${item[3]}`;
        changed = true;
        continue;
      }
      // a non-list, same-or-lower-indent line ends the expose block
      const keyish = line.match(/^(\s*)\S/);
      const isListItem = /^\s*-\s/.test(line);
      if (line.trim() !== "" && !isListItem && keyish && keyish[1].length <= exposeIndent) {
        inExpose = false;
      } else if (isListItem && keyish && keyish[1].length <= exposeIndent) {
        inExpose = false;
      }
    }
  }
  if (changed) {
    fs.writeFileSync(file, lines.join("\n"));
    stats.composeExpose++;
  }
}

if (!fs.existsSync(SUITE)) {
  console.error(`suite not found: ${SUITE}`);
  process.exit(1);
}

for (const taskDir of listDirs(SUITE)) {
  for (const df of findFiles(taskDir, (n) => /^Dockerfile/i.test(n))) {
    patchDockerfile(df);
  }
  for (const cf of findFiles(taskDir, (n) => /^docker-compose\.ya?ml$/i.test(n))) {
    patchCompose(cf);
  }
}

console.log(
  `patched: dockerfiles=${stats.dockerfiles} ` +
    `(archive=${stats.archive}, phantomjs=${stats.phantomjs}, ` +
    `composer-pin=${stats.composerPin}), ` +
    `compose-expose=${stats.composeExpose}`,
);
