# XBOW-104 build-infra notes (time-rot fixes)

**Read this before running the suite.** Several XBEN challenges stopped building
over time — not because of the agent or the model backbone, but because the
upstream base images and mirrors aged out. Symptoms show up as `BUILD FAILED` /
`UP FAILED` in the runner log and land in `excluded_attempts` (they never reach
the agent). Run the patch script first:

```powershell
# Only after active runs/builds sharing the suite have finished.
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/xbow104/patch-suite.mjs')
```

The script edits the **gitignored** suite under
`benchmarks/xbow-validation-benchmarks/` in place (local-only, never committed),
and leaves marker comments (`# xbow-patch:...`) so it can replace its own prior
output. It changes build plumbing only — never the challenge or flag logic.

## The failure classes

### 1. `apt-get update` → exit 100 (EOL Debian mirrors 404) — FIXABLE
Old bases (`python:3.8-slim-buster`, `php:7.1-apache` = stretch, `httpd:2.4.x`)
pin Debian **buster/stretch**, which reached EOL and were removed from
`deb.debian.org` (404 on the `Release` file). They now live on
`archive.debian.org`.
- **Fix:** for a Dockerfile whose `sources.list` names an archived release
  (`buster|stretch|jessie|wheezy`), repoint `deb.debian.org` **and**
  `security.debian.org` → `archive.debian.org`, drop the `*-updates` suite
  (not archived), and set `Acquire::Check-Valid-Until "false"`.
- **Important:** do this ONLY for archived releases. `bullseye`/`bookworm` are
  still live on `deb.debian.org` + `security.debian.org` — repointing them to
  archive 404s, and dropping their security suite causes a version skew
  (`perl : Depends: perl-base (= …u3) but …u5 is to be installed`) because the
  base image already carries security-patched packages. The script gates on the
  codename in `sources.list` for exactly this reason.

### 2. `apt-get install phantomjs` → exit 100 (package removed) — FIXABLE
`phantomjs` was dropped from Debian entirely, so ~23 XSS challenges on
`python:2.7.18-slim` fail. It is not in the archive either.
- **Fix:** replace the phantomjs apt install with the upstream **2.1.1 static
  binary** (`bitbucket.org/ariya/phantomjs/.../phantomjs-2.1.1-linux-x86_64`) →
  `/usr/local/bin/phantomjs`, pulling `libfontconfig1`+`bzip2` via apt (which
  works once fix #1 repoints buster to archive). These Dockerfiles already set
  `ENV OPENSSL_CONF=/dev/null`, which the 2.1.1 binary needs at runtime — do not
  remove it.

### 3. `invalid start port 'N:M'` (bad `expose:` syntax) — FIXABLE
Several compose files write `expose: - 3306:3306`. `expose` takes a **container
port only**; the `host:container` mapping form is invalid and newer compose
rejects it (`invalid start port '3306:3306': invalid syntax`).
- **Fix:** rewrite `expose: - N:M` → `expose: - M` (keep the container port).
  `ports:` mappings are left untouched — those are valid.

### 4. `failed to load cache key: "" failed validation` (old image) — INVESTIGATE
Ancient bases like `mysql:5.7.15` (used by the `db` service in the IDOR-style
challenges) fail at `FROM` with `"" failed validation` /
`content digest … not found`. A corrupt/incomplete local content-store layer is
one possible cause; the error alone does not prove it. Retain the failed build
log, image reference/digest and Docker inspection result before classifying it.
The patch script does not modify this case.
- During a live run, do not force-delete a shared image, prune the builder, or
  restart Docker. Those actions can break unrelated tasks.
- When all affected runs/builds are idle, the owner may remove only an unused
  affected reference without force and re-pull that exact reference. If it is
  still in use, stop cleanup and investigate ownership.
- Verify by retrying only the failed task through the harness. Application and
  runner images must use `scripts/dimage.ps1`; do not bypass it with docker build.

### 5. `composer install` exit 2 (advisory-blocked packages) — FIXABLE
Recent `composer:latest` refuses to install dependencies that carry a published
security advisory: `… twig/twig[v1.19.0] … not loaded, because they are affected
by security advisories …` → `composer install` exits 2. The challenges pin old,
deliberately-vulnerable deps on purpose, so the block is spurious here. Only the
composer **builder stage** version matters (the base image is unaffected).
- **Fix:** pin the composer stage to a pre-block release — the script rewrites
  `composer:latest` → `composer:2.7` in any Dockerfile. 2.7 predates the
  install-time advisory block and supports php 7.2+, so it covers every
  composer-using challenge here. (Targets: XBEN-044; also applies to XBEN-092.)

### 6. XBEN-020 `no published port` — HARNESS TARGET SELECTION

The public entrypoint `ssrf-demo-app` already has `ports: ["80"]`.
`internal-service` intentionally has only `expose: ["80"]`: the solver must
reach it through SSRF. Compose can list the internal dependency first. Older
runners chose the first non-database service, so Claude could not reach it from
the host and native solvers could target the private service directly.

The shared `target.mjs` selects a configured non-infrastructure service with a
published port before falling back to an internal-only application. All three
runners use it. **Do not publish internal-service or alter challenge logic.**
Retain previous attempts and retry XBEN-020 from the public entrypoint after
the current shared-suite runs finish. Existing processes do not hot-reload fixes.

### 7. False diagnostics and lifecycle failures — HARNESS

- An active benchmark network is expected, not orphaned. An empty network needs
  ownership checks; Docker query failure means unknown, not empty.
- A watchdog must match its wrapper token and exact attempt and must not clean
  up while the owning runner is alive. A task name alone is not ownership.
- Cancellation stops scheduling new tasks. Timeout cleanup must complete before
  evidence/manifest finalization.
- Provider runners stop if patch-suite fails; they do not start a broken suite.
- Concurrency must be an integer 1–5; timeout must be positive and finite.
- Build the runner through dimage's base → app → runner dependency graph. This
  avoids silently inheriting a stale `minimal-agent:check` image and uses the
  default Docker builder with direct image-store loading.

## Housekeeping during a run

Let each runner remove its own finished task's project resources and exact
image references. Do not use global image/builder/volume prune, wildcard
container deletion, shared image-ID force removal, or Docker restart while runs
are active. Identical image IDs may be shared by different task tags.
Never delete losing attempts to reclaim space: they carry retry/cost provenance.

## What stays broken (upstream, out of scope here)
A handful of challenges have genuinely unbuildable upstreams beyond mirror/image
rot (e.g. sources that no longer resolve at all). Those remain in
`excluded_attempts`; that is an honest exclusion, not an agent failure.

## Backbone note (unrelated to builds)
The Rust runtime reads `OPENAI_*` only (never `ANTHROPIC_*`). z.ai GLM runs via
`OPENAI_BASE_URL=https://api.z.ai/api/coding/paas/v4`, `OPENAI_MODEL=glm-5.3-flash`.
Keep combined active task concurrency at **5 or below**, and lower it if the
selected provider requires it. A transient 429 followed by normal completion
is not an infrastructure exclusion. Check the actual terminal outcome.
Missing tool/wait/safety telemetry is **unmeasured**, not zero failures.
See [RUNBOOK.md](RUNBOOK.md) for reporting and escalation rules.
