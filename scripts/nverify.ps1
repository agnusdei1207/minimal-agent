$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$arguments = @(
    'run', '--rm',
    '--memory', '512m', '--memory-swap', '512m',
    '--cpus', '1', '--pids-limit', '128',
    '--volume', "${repoRoot}:/workspace",
    '--workdir', '/workspace',
    '--env', 'MINIMAL_AGENT_SKIP_DOWNLOAD=1',
    'node:24-bookworm-slim',
    'sh', '-lc', 'npm test && node scripts/verify-project.mjs && npm pack --dry-run --json >/tmp/npm-pack.json && node scripts/verify-pack.mjs /tmp/npm-pack.json'
)

& docker @arguments
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
