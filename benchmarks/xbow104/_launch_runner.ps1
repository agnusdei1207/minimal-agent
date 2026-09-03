Set-Location 'C:\workspace\minimal-agent'
Get-Content .env | ForEach-Object { if ($_ -match '^([A-Z_]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') } }
$env:MINIMAL_AGENT_PROVIDER_TIMEOUT='300'
$env:OPENAI_TIMEOUT='300'
$out = 'C:\Users\lepisode\AppData\Local\Temp\xbow_runner.log'
$err = 'C:\Users\lepisode\AppData\Local\Temp\xbow_runner.err.log'
$p = Start-Process -FilePath 'node' -ArgumentList 'benchmarks/xbow104/runner.mjs','--all','--concurrency','1','--timeout','900' -WorkingDirectory 'C:\workspace\minimal-agent' -RedirectStandardOutput $out -RedirectStandardError $err -WindowStyle Hidden -PassThru
Write-Output ('started detached pid ' + $p.Id)
