param(
    [ValidateSet('base', 'app', 'all', 'runner')]
    [string] $Target = 'all',
    [string] $Tag = 'pentesting:0.200.1',
    [string] $BaseTag = 'agnusdei1207/pentesting-runtime-base:latest',
    [switch] $Push,
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

# Build with the daemon's local docker builder rather than a memory-capped
# docker-container builder. Select the appropriate builder name matching
# the active context ('desktop-linux' for Docker Desktop, 'default' otherwise).
$builder = 'default'
if ($env:DOCKER_BUILDER) {
    $builder = $env:DOCKER_BUILDER
} else {
    $dockerCfg = Join-Path $env:USERPROFILE '.docker\config.json'
    if (Test-Path $dockerCfg) {
        try {
            $cfg = Get-Content $dockerCfg -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json -ErrorAction SilentlyContinue
            if ($cfg.currentContext -eq 'desktop-linux') {
                $builder = 'desktop-linux'
            }
        } catch { }
    }
}

Push-Location $repoRoot
try {
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

    # Push to registry when requested (e.g. npm run docker:base)
    if ($Push) {
        $pushTarget = if ($Target -eq 'base') { $BaseTag } else { $Tag }
        Write-Host "Pushing $pushTarget to registry..."
        & docker push $pushTarget
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    }
}
finally {
    Pop-Location
}
