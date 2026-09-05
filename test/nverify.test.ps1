$ErrorActionPreference = 'Stop'
$global:nverifyCalls = [System.Collections.Generic.List[object]]::new()
$global:nverifyNodeCalls = [System.Collections.Generic.List[object]]::new()
$global:nverifyNodeVersion = 'v24.20.0'
function global:docker {
    $global:nverifyCalls.Add(@($args))
    $global:LASTEXITCODE = 0
}
function global:node {
    $global:nverifyNodeCalls.Add(@($args))
    $global:LASTEXITCODE = 0
    if ($args[0] -eq '--version') { $global:nverifyNodeVersion }
}
try {
    & (Join-Path $PSScriptRoot '..\scripts\nverify.ps1') -NodeArguments @('--test', 'test/xbow-target.test.mjs')
    $call = $global:nverifyCalls[0]
    if (($call[($call.Count - 3)..($call.Count - 1)] -join '|') -ne 'node|--test|test/xbow-target.test.mjs') {
        throw 'explicit Node arguments must run directly under Node 24 without shell interpolation'
    }
    if ($call -notcontains 'node:24-bookworm-slim') { throw 'Node 24 image required' }
    & (Join-Path $PSScriptRoot '..\scripts\nverify.ps1')
    $default = $global:nverifyCalls[1][-1]
    if ($default -ne 'npm test && node scripts/verify-project.mjs && npm pack --dry-run --json >/tmp/npm-pack.json && node scripts/verify-pack.mjs /tmp/npm-pack.json') {
        throw 'default verification gate must remain intact'
    }
    $literal = 'spaces and $(literal); stay literal'
    & (Join-Path $PSScriptRoot '..\scripts\nverify.ps1') -HostNode -NodeArguments @('fixture.mjs', $literal)
    if ($global:nverifyCalls.Count -ne 2) { throw 'explicit host mode must not launch Docker' }
    if (($global:nverifyNodeCalls[0] -join '|') -ne '--version') { throw 'host Node version must be checked first' }
    if (($global:nverifyNodeCalls[1] -join '|') -ne "fixture.mjs|$literal") { throw 'host arguments must be passed literally' }
    foreach ($badVersion in @('v22.1.0', 'v25.0.0', 'v24.0.0-nightly', 'unexpected')) {
        $global:nverifyNodeVersion = $badVersion
        $before = $global:nverifyNodeCalls.Count
        $rejected = $false
        try { & (Join-Path $PSScriptRoot '..\scripts\nverify.ps1') -HostNode -NodeArguments @('fixture.mjs') }
        catch { $rejected = $_.Exception.Message -match 'Node 24' }
        if (-not $rejected -or $global:nverifyNodeCalls.Count -ne ($before + 1)) {
            throw 'unsupported host Node must be rejected before executing the requested script'
        }
    }
    $before = $global:nverifyNodeCalls.Count
    $rejected = $false
    try { & (Join-Path $PSScriptRoot '..\scripts\nverify.ps1') -HostNode }
    catch { $rejected = $_.Exception.Message -match 'NodeArguments' }
    if (-not $rejected -or $global:nverifyNodeCalls.Count -ne $before) { throw 'host mode requires explicit arguments' }
    'nverify wrapper tests passed'
}
finally {
    Remove-Item Function:\docker -ErrorAction SilentlyContinue
    Remove-Item Function:\node -ErrorAction SilentlyContinue
    Remove-Variable nverifyCalls -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable nverifyNodeCalls -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable nverifyNodeVersion -Scope Global -ErrorAction SilentlyContinue
}
