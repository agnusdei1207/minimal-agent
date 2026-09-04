# XBOW-104 build-infra notes (time-rot fixes)

**Read this before running the suite.** Several XBEN challenges stopped building
over time — not because of the agent or the model backbone, but because the
upstream base images and mirrors aged out. Symptoms show up as `BUILD FAILED` /
`UP FAILED` in the runner log and land in `excluded_attempts` (they never reach
the agent). Run the patch script first:

```bash
node benchmarks/xbow104/patch-suite.mjs   # idempotent; safe to re-run
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

### 4. `failed to load cache key: "" failed validation` (old image) — DOCKER-SIDE
Ancient bases like `mysql:5.7.15` (used by the `db` service in the IDOR-style
challenges) fail at `FROM` with `"" failed validation` /
`content digest … not found`. This is a **corrupt/incomplete layer in the local
containerd content store**, not a Dockerfile problem — the script does NOT touch
it.
- **Fix (docker-side, no file edit):**
  ```bash
  docker rmi -f mysql:5.7.15            # drop the bad tag
  docker image prune -f                 # GC the dangling/corrupt layer
  docker pull mysql:5.7.15              # re-fetch clean
  ```
  If a layer is still cached as "Already exists" and the digest stays missing,
  restart the Docker engine (rebuilds the content-store index) and re-pull.
  Verify with `docker build <task>/mysql`.

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

## Housekeeping during a run
Old base images and per-task target images/volumes pile up fast. Between tasks
(or between runs) reclaim space **without touching what's in use**:
```bash
docker image prune -f          # dangling images
docker volume prune -f         # unused volumes
docker builder prune -f        # build cache
```
Do NOT run `docker system prune -a --volumes` while a run is live — it removes
base images the remaining tasks still need and can break in-flight builds.

## What stays broken (upstream, out of scope here)
A handful of challenges have genuinely unbuildable upstreams beyond mirror/image
rot (e.g. sources that no longer resolve at all). Those remain in
`excluded_attempts`; that is an honest exclusion, not an agent failure.

## Backbone note (unrelated to builds)
The Rust runtime reads `OPENAI_*` only (never `ANTHROPIC_*`). z.ai GLM runs via
`OPENAI_BASE_URL=https://api.z.ai/api/coding/paas/v4`, `OPENAI_MODEL=glm-5.3-flash`.
Keep `--concurrency` at **5 or below**: at 10 the provider returns 429
(`code 1302`) which cascades into `RUNTIME_FAULT` exclusions. See the run-state
memory for details.
