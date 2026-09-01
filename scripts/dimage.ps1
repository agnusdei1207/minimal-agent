param(
    [ValidateSet('base', 'app', 'all')]
    [string] $Target = 'all',
    [string] $Tag = 'minimal-agent:0.110.0'
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$bakeFile = Join-Path $repoRoot 'docker-bake.hcl'

# The `app` target pulls in `base` automatically as a build dependency
# (app.Dockerfile `FROM runtime-base`, resolved via the bake `target:base`
# context), so building only `app` for the full pipeline still builds base.
[string[]]$bakeTargets = switch ($Target) {
    'base' { @('base') }
    'app' { @('app') }
    'all' { @('app') }
}

# Build with the default (docker) builder rather than a memory-capped
# docker-container builder. The docker driver writes straight into the local
# image store, so there is NO OCI export + tarball + re-import step — which is
# exactly what ran the capped BuildKit container out of memory and killed large
# image builds intermittently (graceful_stop / EOF). Compile parallelism (the
# heaviest phase) stays bounded via CARGO_BUILD_JOBS=2 in app.Dockerfile.
Push-Location $repoRoot
try {
    [string[]]$bakeArgs = @(
        'buildx', 'bake',
        '--file', $bakeFile,
        '--provenance=false',
        '--sbom=false',
        '--load',
        '--set', "app.tags=$Tag"
    )
    $bakeArgs += $bakeTargets
    & docker @bakeArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}
finally {
    Pop-Location
}
