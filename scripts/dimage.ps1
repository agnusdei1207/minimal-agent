param(
    [ValidateSet('base', 'app', 'all', 'runner')]
    [string] $Target = 'all',
    [string] $Tag = 'minimal-agent:0.110.0',
    [switch] $NoCache
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$bakeFile = Join-Path $repoRoot 'docker-bake.hcl'

# The `app` target pulls in `base` automatically as a build dependency
# (app.Dockerfile `FROM runtime-base`, resolved via the bake `target:base`
# context), so building only `app` for the full pipeline still builds base.
# The benchmark `runner` similarly depends on app, then base.
[string[]]$bakeTargets = switch ($Target) {
    'base' { @('base') }
    'app' { @('app') }
    'all' { @('app') }
    'runner' { @('runner') }
}

# Build with the default (docker) builder rather than a memory-capped
# docker-container builder. The docker driver writes straight into the local
# image store, so there is NO OCI export + tarball + re-import step — which is
# exactly what ran the capped BuildKit container out of memory and killed large
# image builds intermittently (graceful_stop / EOF). Compile parallelism (the
# heaviest phase) stays bounded via CARGO_BUILD_JOBS=2 in app.Dockerfile.
Push-Location $repoRoot
try {
    $currentContext = (& docker context show 2>$null).Trim()
    $builder = if ($currentContext -and (& docker buildx inspect $currentContext 2>$null)) { $currentContext } else { 'default' }
    [string[]]$bakeArgs = @(
        'buildx', 'bake',
        '--builder', $builder,
        '--file', $bakeFile,
        '--provenance=false',
        '--sbom=false',
        '--load'
    )
    if ($Target -eq 'runner') {
        if (-not $PSBoundParameters.ContainsKey('Tag')) { $Tag = 'xbow-agent-runner:latest' }
        $bakeArgs += @('--set', "runner.tags=$Tag")
    }
    else {
        $bakeArgs += @('--set', "app.tags=$Tag")
    }
    if ($NoCache) { $bakeArgs += '--no-cache' }
    $bakeArgs += $bakeTargets
    & docker @bakeArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}
finally {
    Pop-Location
}
