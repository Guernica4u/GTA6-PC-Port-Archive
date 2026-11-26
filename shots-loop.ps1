for($i=0;$i -lt 40;$i++){
  if(-not (Get-Process | ? { $_.Name -like 'victory*' -and $_.MainWindowHandle -ne 0 })){ break }
  powershell -NoProfile -ExecutionPolicy Bypass -File C:\Users\Vyto\forks\coolfox\shot.ps1 -Name t22auto | Out-Null
  Start-Sleep -Seconds 30
}
