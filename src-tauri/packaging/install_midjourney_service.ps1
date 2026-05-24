Param(
    [string]$ResourceDir
)
if (-not $ResourceDir) {
    Write-Host "Usage: .\install_midjourney_service.ps1 -ResourceDir C:\path\to\resources"
    exit 1
}
$run = Join-Path $ResourceDir 'midjourney-proxy\run_app.sh'
if (-not (Test-Path $run)) { Write-Host "run_app.sh not found at $run"; exit 1 }
$Action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -WindowStyle Hidden -Command \"Start-Process -FilePath 'sh' -ArgumentList '$run' -WindowStyle Hidden\""
$Trigger = New-ScheduledTaskTrigger -AtLogOn
$Principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive
Register-ScheduledTask -TaskName "MidjourneyProxy" -Action $Action -Trigger $Trigger -Principal $Principal -RunLevel Limited -Force
Write-Host "Registered scheduled task 'MidjourneyProxy' to run on user login."
