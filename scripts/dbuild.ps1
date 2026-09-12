param()

$ErrorActionPreference = 'Stop'
$CargoArgs = @($args)
if (-not $CargoArgs -or $CargoArgs.Count -eq 0) {
    throw 'usage: scripts/dbuild.ps1 <cargo arguments>'
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$dockerArgs = @(
    'run', '--rm',
    '--memory', '2g', '--memory-swap', '2g',
    '--cpus', '2', '--pids-limit', '512',
    '--volume', "${repoRoot}:/workspace",
    '--volume', 'pentesting-cargo-registry:/usr/local/cargo/registry',
    '--volume', 'pentesting-target:/workspace/target',
    '--workdir', '/workspace',
    '--env', 'CARGO_BUILD_JOBS=2',
    'rust:1.98-bookworm',
    'cargo'
) + $CargoArgs

& docker @dockerArgs
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
