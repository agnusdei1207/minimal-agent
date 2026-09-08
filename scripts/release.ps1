param(
    [string] $Version = '0.200.1',
    [switch] $Push,
    [switch] $DryRun,
    [switch] $SkipDocker,
    [switch] $SkipNpm
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "  PENTESTING RELEASE ORCHESTRATION PIPELINE" -ForegroundColor Cyan
Write-Host "  Target Version: $Version" -ForegroundColor Yellow
Write-Host "  Mode: $(if ($Push) { 'PUBLISH / PUSH' } else { 'LOCAL BUILD / VERIFY' })" -ForegroundColor Yellow
Write-Host "================================================================" -ForegroundColor Cyan

# 1. Verification Gate: Node & Packaging
Write-Host "`n[1/5] Running Node 24 & Packaging verification..." -ForegroundColor Green
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'nverify.ps1')
if ($LASTEXITCODE -ne 0) { throw "Node verification failed with code $LASTEXITCODE" }

# 2. Verification Gate: Rust Tests, Clippy & Format
Write-Host "`n[2/5] Running Rust 1.98 containerized tests and clippy..." -ForegroundColor Green
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'dbuild.ps1') test
if ($LASTEXITCODE -ne 0) { throw "Rust test suite failed with code $LASTEXITCODE" }

& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'dbuild.ps1') clippy --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw "Rust clippy failed with code $LASTEXITCODE" }

& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'dbuild.ps1') fmt --check
if ($LASTEXITCODE -ne 0) { throw "Rust format check failed with code $LASTEXITCODE" }

# 3. Docker Image Build Gate
if (-not $SkipDocker) {
    Write-Host "`n[3/5] Building Docker runtime-base and app images..." -ForegroundColor Green
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'dimage.ps1') -Target base
    if ($LASTEXITCODE -ne 0) { throw "Docker base build failed with code $LASTEXITCODE" }

    $appTag = "agnusdei1207/pentesting:$Version"
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'dimage.ps1') -Target app -Tag $appTag
    if ($LASTEXITCODE -ne 0) { throw "Docker app build failed with code $LASTEXITCODE" }

    docker tag $appTag "agnusdei1207/pentesting:latest"
    Write-Host "Tagged: agnusdei1207/pentesting:latest" -ForegroundColor Gray
} else {
    Write-Host "`n[3/5] Skipping Docker build as requested." -ForegroundColor DarkGray
}

# 4. Dry-run Check
if ($DryRun) {
    Write-Host "`n[DRY RUN] All builds and gates succeeded. Push was not executed." -ForegroundColor Magenta
    exit 0
}

# 5. Push and Publish
if ($Push) {
    Write-Host "`n[4/5] Pushing Docker images to Docker Hub..." -ForegroundColor Green
    if (-not $SkipDocker) {
        docker push "agnusdei1207/pentesting-runtime-base:latest"
        docker push "agnusdei1207/pentesting:$Version"
        docker push "agnusdei1207/pentesting:latest"
    }

    if (-not $SkipNpm) {
        Write-Host "`n[5/5] Publishing package to npm..." -ForegroundColor Green
        $published = (npm view "pentesting@$Version" version 2>$null)
        if ($published -eq $Version) {
            Write-Host "pentesting@$Version is already published on npm. Skipping duplicate publish." -ForegroundColor Yellow
        } else {
            npm publish --access public
            if ($LASTEXITCODE -ne 0) { throw "npm publish failed with code $LASTEXITCODE" }
        }
    }

    Write-Host "`n================================================================" -ForegroundColor Cyan
    Write-Host "  PENTESTING v$Version RELEASE COMPLETED SUCCESSFULLY!" -ForegroundColor Green
    Write-Host "================================================================" -ForegroundColor Cyan
} else {
    Write-Host "`n================================================================" -ForegroundColor Cyan
    Write-Host "  Local release build & verification PASSED 100%!" -ForegroundColor Green
    Write-Host "  To push to Docker Hub & npm, run with: -Push" -ForegroundColor Yellow
    Write-Host "================================================================" -ForegroundColor Cyan
}
