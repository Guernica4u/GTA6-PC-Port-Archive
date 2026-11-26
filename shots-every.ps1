param([string]$Name="run",[int]$Every=10)
while (Get-Process | Where-Object { $_.Name -like 'victory*' -and $_.MainWindowHandle -ne 0 }) {
  powershell -NoProfile -ExecutionPolicy Bypass -File C:\Users\Vyto\forks\coolfox\shot.ps1 -Name $Name | Out-Null
  Start-Sleep -Seconds $Every
}
