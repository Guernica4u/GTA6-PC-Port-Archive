param([string]$Name = "shot")
# Captures the VIctory game window without focusing it; prints the title and the saved path.
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;using System.Runtime.InteropServices;
public class PW{[DllImport("user32.dll")]public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);[DllImport("user32.dll")]public static extern bool GetClientRect(IntPtr h,out RECT r);public struct RECT{public int L,T,R,B;}}
"@
$pr = Get-Process | Where-Object { $_.Name -like 'victory*' -and $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $pr) { "game not running"; exit 1 }
$pr.MainWindowTitle
$r = New-Object PW+RECT
[PW]::GetClientRect($pr.MainWindowHandle, [ref]$r) | Out-Null
$bmp = New-Object System.Drawing.Bitmap $r.R, $r.B
$g = [System.Drawing.Graphics]::FromImage($bmp)
$dc = $g.GetHdc(); [PW]::PrintWindow($pr.MainWindowHandle, $dc, 3) | Out-Null; $g.ReleaseHdc($dc)
$s = New-Object System.Drawing.Bitmap $bmp, 1200, ([int](1200 * $r.B / $r.R))
$p = "C:\Users\Vyto\forks\runs\$Name-$(Get-Date -Format HHmmss).jpg"
$s.Save($p, [System.Drawing.Imaging.ImageFormat]::Jpeg)
$p
