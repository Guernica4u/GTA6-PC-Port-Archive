param([string]$Name = "run")
# Waits for the game window, then every 15 s logs GPU use and video memory to
# forks\runs\gpu-<Name>.csv and every 30 s saves a picture of the game window;
# stops when the game closes.
$csv = "C:\Users\Vyto\forks\runs\gpu-$Name.csv"
"time,gpu_percent,vram_used_mb,vram_total_mb,fps_title" | Set-Content $csv
$game = { Get-Process | Where-Object { $_.Name -like 'victory*' -and $_.MainWindowHandle -ne 0 } | Select-Object -First 1 }
for ($w = 0; $w -lt 120 -and -not (& $game); $w++) { Start-Sleep -Seconds 5 }
$i = 0
while ($p = & $game) {
    $q = (& nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits) -replace '\s', ''
    $fps = if ($p.MainWindowTitle -match '([\d.]+) fps') { $Matches[1] } else { '' }
    "$(Get-Date -Format HH:mm:ss),$q,$fps" | Add-Content $csv
    if ($i % 2 -eq 0) {
        powershell -NoProfile -ExecutionPolicy Bypass -File C:\Users\Vyto\forks\coolfox\shot.ps1 -Name "$Name-auto" | Out-Null
    }
    $i++
    Start-Sleep -Seconds 15
}
