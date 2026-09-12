param(
    [string[]]$NodeArguments = @(),
    [switch]$HostNode
)

$ErrorActionPreference = 'Stop'
if ($HostNode) {
    if ($NodeArguments.Count -eq 0) {
        throw '-HostNode requires explicit -NodeArguments'
    }
    $nodeVersion = & node --version
    if ($LASTEXITCODE -ne 0 -or @($nodeVersion).Count -ne 1 -or $nodeVersion -notmatch '^v24\.\d+\.\d+$') {
        throw '-HostNode requires a stable Node 24 executable on PATH'
    }
    & node @NodeArguments
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    return
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$arguments = @(
    'run', '--rm',
    '--memory', '512m', '--memory-swap', '512m',
    '--cpus', '1', '--pids-limit', '128',
    '--volume', "${repoRoot}:/workspace",
    '--workdir', '/workspace',
    '--env', 'PENTESTING_SKIP_DOWNLOAD=1',
    'node:24-bookworm-slim'
)
if ($NodeArguments.Count -gt 0) {
    $arguments += @('node') + $NodeArguments
}
else {
    $arguments += @('sh', '-lc', 'npm test && node scripts/verify-project.mjs && npm pack --dry-run --json >/tmp/npm-pack.json && node scripts/verify-pack.mjs /tmp/npm-pack.json')
}

& docker @arguments
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
