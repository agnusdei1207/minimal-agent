$ErrorActionPreference = 'Stop'
$global:dimageCalls = [System.Collections.Generic.List[object]]::new()
$browserInstaller = [System.IO.File]::ReadAllBytes(
    (Join-Path $PSScriptRoot '..\docker\install-browser.sh')
)
if ($browserInstaller -contains 13) {
    throw 'docker/install-browser.sh must use LF line endings for Bash'
}

function global:docker {
    $global:dimageCalls.Add(@($args))
    if ($args[0] -eq 'buildx' -and $args[1] -eq 'create') {
        'minimal-agent-test-builder'
    }
    $global:LASTEXITCODE = 0
}

try {
    & (Join-Path $PSScriptRoot '..\scripts\dimage.ps1') -Target base

    if ($global:dimageCalls.Count -ne 1) {
        throw "expected 1 buildx bake call; got $($global:dimageCalls.Count)"
    }

    $baseBuild = $global:dimageCalls[0]
    if (($baseBuild -join ' ') -notmatch '^buildx bake .*--load .* base$') {
        throw "base target did not invoke buildx bake with --load: $($baseBuild -join ' ')"
    }
    if (($baseBuild -join ' ') -notmatch '--builder (default|desktop-linux)(?: |$)') {
        throw 'build must explicitly select the local daemon builder regardless of the active buildx selection'
    }

    $global:dimageCalls.Clear()
    & (Join-Path $PSScriptRoot '..\scripts\dimage.ps1') -Target all -Tag 'minimal-agent:test'

    if ($global:dimageCalls.Count -ne 1) {
        throw "expected 1 buildx bake call; got $($global:dimageCalls.Count)"
    }

    $allBuild = $global:dimageCalls[0]
    if (($allBuild -join ' ') -notmatch '--set app.tags=minimal-agent:test app$') {
        throw "all target must build app with custom tag: $($allBuild -join ' ')"
    }

    $global:dimageCalls.Clear()
    & (Join-Path $PSScriptRoot '..\scripts\dimage.ps1') -Target runner -Tag 'xbow-agent-runner:test' -NoCache
    if ($global:dimageCalls.Count -ne 1) {
        throw "runner must use one bake pipeline; got $($global:dimageCalls.Count) calls"
    }
    $runnerBuild = $global:dimageCalls[0]
    if (($runnerBuild -join ' ') -notmatch '--set runner.tags=xbow-agent-runner:test --no-cache runner$') {
        throw "runner must build the runner target with its own tag and no-cache: $($runnerBuild -join ' ')"
    }
    if ($runnerBuild -notcontains '--load') {
        throw 'runner must load straight into the default image store'
    }
}
finally {
    Remove-Item Function:\docker -ErrorAction SilentlyContinue
    Remove-Variable dimageCalls -Scope Global -ErrorAction SilentlyContinue
}
