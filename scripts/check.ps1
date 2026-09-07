$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$image = 'pentesting:check'

& (Join-Path $PSScriptRoot 'dimage.ps1') -Target all -Tag $image
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

# Always start a fresh, isolated run — never resume a prior journal. Resuming an
# earlier run replays accumulated state (goal, transcript, brief) into a new
# session; with a weak model that state drifts and reads as broken. Each
# `npm run check` is therefore a clean session in its own run root. (User
# decision 2026-09-02; ADR-0002 §3.21.)
$runRoot = '/state/check-' + [guid]::NewGuid().ToString('N')

# Provider credentials: prefer the gitignored .env (fixed local config), and
# fall back to forwarding the host's shell variables when no .env is present.
$envFile = Join-Path $repoRoot '.env'
if (Test-Path $envFile) {
    $envArgs = @('--env-file', $envFile)
}
else {
    $envArgs = @(
        '--env', 'OPENAI_API_KEY',
        '--env', 'OPENAI_MODEL',
        '--env', 'OPENAI_BASE_URL',
        '--env', 'OPENAI_CONTEXT_TOKENS',
        '--env', 'OPENROUTER_API_KEY'
    )
}
$arguments = @(
    'run', '-it', '--rm',
    '--memory', '2g', '--memory-swap', '2g',
    '--cpus', '2', '--pids-limit', '512',
    '--volume', 'pentesting-workspace:/workspace',
    '--volume', 'pentesting-state:/state'
) + $envArgs + @(
    $image,
    'run', '--workspace', '/workspace',
    '--goal', 'Help complete the requested task',
    '--run', $runRoot
)

& docker @arguments
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
