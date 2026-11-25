#requires -Version 5.1
<#
.SYNOPSIS
Runs VIctory on Windows and writes a comparable Markdown report outside the source tree.
.DESCRIPTION
Samples the initial process working set and available physical memory every
three seconds. Frame peaks come from VIctory's logged summaries. A missing
sample is marked unobserved. The benchmark filters tracing to the GPU
targets that produce its counters, groups shader refusals by reason and
writes the report under %LOCALAPPDATA%\victory\bench by default.
On normal completion or a limit the script stops descendants assigned to
the benchmark's job. The system-memory floor also applies before launch.
.PARAMETER Baseline
A Markdown report from a previous run. Its six metrics are shown alongside
the new run. Use the same console, scene, cache state and limits to compare.
.EXAMPLE
.\scripts\victory-bench.ps1 -Title 'D:\titles\test.exe' -Console series-s -Label 'baseline'
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Title,
    [string]$Exe = "$PSScriptRoot\..\target\release\victory.exe",
    [ValidateSet('series-s', 'series-x')][string]$Console = 'series-x',
    [ValidateRange(1, 86400)][int]$Seconds = 180,
    [ValidateRange(0.000001, 1048576)][double]$MaxRamGB = 8,
    [ValidateRange(0, 1048576)][double]$MinSysGB = 3,
    [ValidatePattern('^[^\r\n|]{1,120}$')][string]$Label = 'run',
    [string]$Baseline,
    [string]$OutputDirectory = "$env:LOCALAPPDATA\victory\bench"
)

$ErrorActionPreference = 'Stop'
$invariant = [Globalization.CultureInfo]::InvariantCulture

function Format-Number([double]$value, [string]$format) {
    $value.ToString($format, $invariant)
}

function Get-FreeBytes {
    $memory = Get-CimInstance -ClassName Win32_OperatingSystem -Property FreePhysicalMemory
    if ($null -eq $memory.FreePhysicalMemory) { throw 'Windows did not report available physical memory.' }
    [long]$memory.FreePhysicalMemory * 1KB
}

function Read-Event([string]$line) {
    $line = $line -replace '\x1b\[[0-9;]*m', ''
    if ($line -match "^VIctory: unhandled exception 0x[0-9a-f]+ at (.+)$") {
        $stats.Crash = 'unknown'
        if ($Matches[1] -match '^[^+]+\+(0x[0-9a-f]+)\b') { $stats.Crash = $Matches[1] }
    } elseif ($line -match " WARN victory_gpu::pipeline: shader does not translate; its pipelines' draws are dropped(?: |$)") {
        $stats.Translation++
        $reason = 'other translation failure'
        # These messages come from ShaderError and contain static translator text.
        # Strip instruction offsets so different programs group under the same reason.
        if ($line -match ' error=(?:GFX10 code at 0x[0-9a-f]+: |malformed shader: )?([a-zA-Z0-9 ()_,.\x27-]{1,180}?) (?:(?:cached|ms)=)') {
            $reason = $Matches[1]
        }
        $reasons[$reason]++
    } elseif ($line -match ' WARN victory_gpu::resource: no host texture(?: |$)') {
        $stats.Texture++
    } elseif ($line -match ' INFO victory_gpu::host: ([0-9.]+) fps, [0-9.]+ ms/frame .*?, ([0-9]+) draws/frame,') {
        $fps = [double]::Parse($Matches[1], $invariant)
        $draws = [long]$Matches[2]
        $stats.Fps = [Math]::Max([double]$stats.Fps, $fps)
        $stats.Draws = [Math]::Max([long]$stats.Draws, $draws)
    }
}

function Add-Sample($workingSet, [long]$free) {
    $rss = '-'
    if ($null -ne $workingSet) {
        $stats.Peak = [Math]::Max([long]$stats.Peak, [long]$workingSet)
        $rss = Format-Number ($workingSet / 1MB) '0.0'
    }
    $time = Format-Number $clock.Elapsed.TotalSeconds '0.0'
    $available = Format-Number ($free / 1MB) '0.0'
    $timeline.Add("| $time | $rss | $available | $($stats.Translation) | $($stats.Texture) |")
}

# Validate all inputs before starting the process. A report never contains log lines.
$exePath = (Get-Item -LiteralPath $Exe).FullName
$titlePath = (Get-Item -LiteralPath $Title).FullName
$directory = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
for ($entry = [IO.DirectoryInfo]$directory; $null -ne $entry; $entry = $entry.Parent) {
    if ($entry.Exists -and ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'Choose an output directory without junctions or symbolic links.'
    }
    if ([IO.File]::Exists([IO.Path]::Combine($entry.FullName, '.victory-root'))) {
        throw 'Choose an output directory outside the VIctory source tree.'
    }
}

$metrics = [ordered]@{
    'translation failures' = '\d+'; 'texture creation failures' = '\d+'
    'peak draws/frame' = '(?:\d+|unobserved)'; 'peak fps' = '(?:\d+(?:\.\d+)?|unobserved)'
    'peak RSS (MiB)' = '(?:\d+(?:\.\d+)?|unobserved)'; 'crash RVA' = '(?:0x[0-9a-f]+|none|unknown)'
}
$previous = @{}
if ($Baseline) {
    $lines = @(Get-Content -LiteralPath $Baseline)
    if ($lines -notcontains '<!-- victory-bench-v1 -->') { throw 'Unrecognized benchmark baseline.' }
    foreach ($metric in $metrics.Keys) {
        $pattern = '^\| ' + [regex]::Escape($metric) + ' \| (' + $metrics[$metric] + ') \|$'
        $rows = @($lines | Where-Object { $_ -match $pattern })
        if ($rows.Count -ne 1) { throw "Invalid benchmark baseline for $metric." }
        $previous[$metric] = [regex]::Match($rows[0], $pattern).Groups[1].Value
    }
}

# A title can relaunch itself. Collect only the initial run; close out every
# descendant in its own Windows job when it ends or a guard fires.
if (-not ('VictoryBenchJob' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public static class VictoryBenchJob {
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern SafeFileHandle CreateJobObjectW(IntPtr attributes, IntPtr name);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool AssignProcessToJobObject(SafeFileHandle job, IntPtr process);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool TerminateJobObject(SafeFileHandle job, uint code);
    public static SafeFileHandle Create() {
        var job = CreateJobObjectW(IntPtr.Zero, IntPtr.Zero);
        if (job.IsInvalid) throw new Win32Exception();
        return job;
    }
    public static void Attach(SafeFileHandle job, Process process) {
        if (!AssignProcessToJobObject(job, process.Handle)) {
            var error = Marshal.GetLastWin32Error();
            if (!process.HasExited) throw new Win32Exception(error);
        }
    }
    public static void Stop(SafeFileHandle job) {
        if (!TerminateJobObject(job, 1)) throw new Win32Exception();
    }
}
'@
}

$stats = @{ Translation = 0; Texture = 0; Crash = 'none'; Fps = $null; Draws = $null; Peak = $null }
$reasons = @{}
$timeline = [Collections.Generic.List[string]]::new()
$clock = [Diagnostics.Stopwatch]::StartNew()
$free = Get-FreeBytes
$stop = 'minimum free system memory'
if ($free -lt $MinSysGB * 1GB) {
    Add-Sample $null $free
} else {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $exePath
    $start.Arguments = 'run "' + $titlePath + '" --execute --console ' + $Console
    $start.WorkingDirectory = $PWD.Path
    $start.UseShellExecute = $false
    $start.RedirectStandardError = $true
    $start.RedirectStandardOutput = $true
    $start.EnvironmentVariables['VICTORY_LOG'] = 'off,victory_gpu::host=info,victory_gpu::resource=warn,victory_gpu::pipeline=warn'
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    $job = [VictoryBenchJob]::Create()
    $started = $false
    $stopped = $false
    try {
        $clock.Restart()
        $started = $process.Start()
        [VictoryBenchJob]::Attach($job, $process)
        $discard = $process.StandardOutput.BaseStream.CopyToAsync([IO.Stream]::Null)
        $read = $process.StandardError.ReadLineAsync()
        $nextSample = 0
        $stop = ''
        while (!$stop) {
            # Limit each batch so a busy logger cannot starve the memory/time guards.
            for ($i = 0; $i -lt 256 -and $null -ne $read -and $read.IsCompleted; $i++) {
                $line = $read.GetAwaiter().GetResult()
                if ($null -eq $line) { $read = $null; break }
                Read-Event $line
                $read = $process.StandardError.ReadLineAsync()
            }
            if ($process.HasExited) {
                $stop = "process exit $($process.ExitCode)"
            } elseif ($clock.Elapsed.TotalSeconds -ge $Seconds) {
                $stop = 'time limit'
            } elseif ($clock.Elapsed.TotalSeconds -ge $nextSample) {
                try {
                    $process.Refresh()
                    $rss = $process.WorkingSet64
                } catch {
                    if ($process.HasExited) { continue }
                    throw
                }
                if ($process.HasExited) { continue }
                $free = Get-FreeBytes
                Add-Sample $rss $free
                if ($rss -gt $MaxRamGB * 1GB) { $stop = 'maximum process RSS' }
                elseif ($free -lt $MinSysGB * 1GB) { $stop = 'minimum free system memory' }
                $nextSample = $clock.Elapsed.TotalSeconds + 3
            } elseif ($null -eq $read -or !$read.IsCompleted) {
                Start-Sleep -Milliseconds 25
            }
        }
        [VictoryBenchJob]::Stop($job)
        $stopped = $true
        $process.WaitForExit()
        # Pipe buffers may still hold the final batch, including the crash header.
        while ($null -ne $read) {
            $line = $read.GetAwaiter().GetResult()
            if ($null -eq $line) { break }
            Read-Event $line
            $read = $process.StandardError.ReadLineAsync()
        }
        $discard.GetAwaiter().GetResult() | Out-Null
        Add-Sample $null (Get-FreeBytes)
    } finally {
        try {
            if ($started -and !$process.HasExited) { $process.Kill() }
            if (!$stopped) { [VictoryBenchJob]::Stop($job) }
        } finally {
            $job.Dispose()
            $process.Dispose()
        }
    }
}

$values = [ordered]@{
    'translation failures' = $stats.Translation; 'texture creation failures' = $stats.Texture
    'peak draws/frame' = $stats.Draws; 'peak fps' = $stats.Fps
    'peak RSS (MiB)' = $stats.Peak; 'crash RVA' = $stats.Crash
}
if ($null -ne $stats.Peak) { $values['peak RSS (MiB)'] = Format-Number ($stats.Peak / 1MB) '0.00' }
if ($null -ne $stats.Fps) { $values['peak fps'] = Format-Number $stats.Fps '0.0' }
foreach ($metric in $metrics.Keys) {
    if ($null -eq $values[$metric]) { $values[$metric] = 'unobserved' }
}

$report = @(
    '# VIctory benchmark'
    '<!-- victory-bench-v1 -->'
    ''
    "Build label: $Label"
    ''
    "Console: $Console. Time budget: $Seconds s. RSS limit: $MaxRamGB GiB. Free-memory floor: $MinSysGB GiB."
    ''
    "Stop: $stop."
    ''
    '| Metric | Value |'
    '| --- | ---: |'
    foreach ($metric in $metrics.Keys) { "| $metric | $($values[$metric]) |" }
    ''
    '## Translation failures by reason'
    ''
    if (!$reasons.Count) { 'None recorded.' }
    foreach ($reason in ($reasons.Keys | Sort-Object)) { "- $($reasons[$reason]) x $reason" }
    ''
    '## Timeline'
    ''
    '| Elapsed (s) | RSS (MiB) | Available (MiB) | Translation failures | Texture failures |'
    '| ---: | ---: | ---: | ---: | ---: |'
    $timeline
    if ($Baseline) {
        ''
        '## Baseline comparison'
        ''
        '| Metric | Baseline | This run |'
        '| --- | ---: | ---: |'
        foreach ($metric in $metrics.Keys) { "| $metric | $($previous[$metric]) | $($values[$metric]) |" }
    }
)
[void][IO.Directory]::CreateDirectory($directory)
$file = Join-Path $directory ("report-{0:yyyyMMdd-HHmmssfff}-{1}.md" -f [DateTime]::UtcNow, [guid]::NewGuid().ToString('N'))
[IO.File]::WriteAllLines($file, [string[]]$report)
$file
