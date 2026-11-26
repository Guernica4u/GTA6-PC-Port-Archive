param([string]$Keys = "C", [int]$Ms = 2000)
# Focuses the VIctory game window and holds the given keys (comma list of
# letters or names: Enter, Esc, Tab, Up, Down, Left, Right, Space) for $Ms
# milliseconds. The layer reads the keyboard only while the game is in front.
Add-Type @"
using System; using System.Runtime.InteropServices;
public class Kb {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint type);
}
"@
$p = Get-Process | Where-Object { $_.Name -like 'victory*' -and $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { "game not running"; exit 1 }
[Kb]::ShowWindow($p.MainWindowHandle, 9) | Out-Null
[Kb]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 200
$named = @{ Enter = 0x0D; Esc = 0x1B; Tab = 0x09; Up = 0x26; Down = 0x28; Left = 0x25; Right = 0x27; Space = 0x20; Back = 0x08 }
$vks = foreach ($k in $Keys.Split(',')) { $k = $k.Trim(); if ($named.ContainsKey($k)) { [byte]$named[$k] } else { [byte][char]$k.ToUpper() } }
foreach ($vk in $vks) { [Kb]::keybd_event($vk, [byte][Kb]::MapVirtualKey($vk, 0), 0, [UIntPtr]::Zero) }
Start-Sleep -Milliseconds $Ms
foreach ($vk in $vks) { [Kb]::keybd_event($vk, [byte][Kb]::MapVirtualKey($vk, 0), 2, [UIntPtr]::Zero) }
"held $Keys for $Ms ms"
