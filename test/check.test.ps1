$ErrorActionPreference = 'Stop'
$global:checkCalls = [System.Collections.Generic.List[object]]::new()

function global:docker {
    $global:checkCalls.Add(@($args))
    if ($args[0] -eq 'buildx' -and $args[1] -eq 'create') {
        'minimal-agent-test-builder'
    }
    if ($args[0] -eq 'run' -and $args -contains '--entrypoint') {
        $global:LASTEXITCODE = 11
    }
    else {
        $global:LASTEXITCODE = 0
    }
}

try {
    & (Join-Path $PSScriptRoot '..\scripts\check.ps1')

    $launch = $global:checkCalls[$global:checkCalls.Count - 1]
    $joined = $launch -join ' '
    if ($joined -notmatch 'minimal-agent:check run .*--run /state/check-[0-9a-f]+$') {
        throw "an active journal writer must launch an isolated run: $joined"
    }
    if ($launch -contains '--resume') {
        throw "an active journal writer was resumed: $joined"
    }
}
finally {
    Remove-Item Function:\docker -ErrorAction SilentlyContinue
    Remove-Variable checkCalls -Scope Global -ErrorAction SilentlyContinue
}
