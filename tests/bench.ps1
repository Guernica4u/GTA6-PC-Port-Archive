#requires -Version 5.1
# Integration test: an ordinary native process writes the runtime's diagnostic
# formats. No title, GPU, extra PowerShell modules or performance counters needed.
$ErrorActionPreference = 'Stop'
$bench = Join-Path $PSScriptRoot '..\scripts\victory-bench.ps1'
$temp = Join-Path ([IO.Path]::GetTempPath()) ('victory bench ' + [guid]::NewGuid())
[void][IO.Directory]::CreateDirectory($temp)

function Expect([string]$report, [string]$line) {
    if (($report -split '\r?\n') -notcontains $line) { throw "Missing '$line' in report." }
}

try {
    $source = Join-Path $temp child.rs
    $exe = Join-Path $temp 'synthetic victory.exe'
    $title = Join-Path $temp 'synthetic title.exe'
    [IO.File]::WriteAllText($title, '')
    @'
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 5);
    assert_eq!(args[0], "run");
    assert!(args[1].ends_with("synthetic title.exe"));
    assert_eq!(args[2..4], ["--execute", "--console"]);
    assert_eq!(args[4], "series-s");
    let mode = std::fs::read_to_string(&args[1]).unwrap();
    if mode == "wait" {
        std::thread::sleep(std::time::Duration::from_secs(30));
        return;
    }
    // More than a pipe buffer, with unreportable text on both streams.
    for _ in 0..4096 {
        println!("private-stdout");
        eprintln!("private-stderr");
    }
    for offset in [8, 16] {
        eprintln!(" WARN victory_gpu::pipeline: shader does not translate; its pipelines' draws are dropped stage=compute hash=123 error=GFX10 code at 0x{offset:x}: a synthetic operation is not supported cached=false ms=1");
    }
    eprintln!(" WARN victory_gpu::resource: no host texture error=private-context");
    eprintln!(" INFO victory_gpu::host: 7.5 fps, 133.3 ms/frame (5.0 preparing), 18 draws/frame, dropped 3");
    eprintln!("\x1b[32m INFO\x1b[0m victory_gpu::host: 30.1 fps, 33.2 ms/frame (1.0 preparing), 2 draws/frame, dropped 0");
    eprintln!(" INFO victory_gpu::bindless: heap views: 150 draws/frame index the heap");
    eprintln!("VIctory: unhandled exception 0x80000003 at private-image.dll+0x4f");
    std::process::exit(7);
}
'@ | Set-Content -LiteralPath $source -Encoding ASCII
    & rustc $source -o $exe
    if ($LASTEXITCODE) { throw 'Could not build the synthetic runner.' }

    $options = @{ Title = $title; Exe = $exe; Console = 'series-s'; OutputDirectory = $temp; MinSysGB = 0; Seconds = 20 }
    $first = & $bench @options
    $text = [IO.File]::ReadAllText($first)
    foreach ($line in @('Stop: process exit 7.', '| peak draws/frame | 18 |', '| peak fps | 30.1 |',
        '| translation failures | 2 |', '| texture creation failures | 1 |', '| crash RVA | 0x4f |',
        '- 2 x a synthetic operation is not supported')) {
        Expect $text $line
    }
    if ($text -match 'private-|synthetic title.exe') { throw 'Raw child output leaked into the report.' }

    [IO.File]::WriteAllText($title, 'wait')
    $options.Seconds = 1
    $timed = [IO.File]::ReadAllText((& $bench @options -Baseline $first))
    Expect $timed 'Stop: time limit.'
    Expect $timed '| peak fps | unobserved |'
    Expect $timed '| peak fps | 30.1 | unobserved |'
    Expect $timed '| crash RVA | 0x4f | none |'

    $options.Seconds = 20
    $limited = [IO.File]::ReadAllText((& $bench @options -MaxRamGB 0.000001))
    Expect $limited 'Stop: maximum process RSS.'
    $options.MinSysGB = 1048576
    $preflight = [IO.File]::ReadAllText((& $bench @options))
    Expect $preflight 'Stop: minimum free system memory.'
    Expect $preflight '| peak RSS (MiB) | unobserved |'

    $bad = Join-Path $temp 'bad.md'
    [IO.File]::WriteAllText($bad, ($text -replace 'peak draws/frame \| 18', 'peak draws/frame | bad'))
    try {
        & $bench @options -Baseline $bad
        throw 'Accepted a malformed baseline.'
    } catch {
        if ($_.Exception.Message -notlike 'Invalid benchmark baseline*') { throw }
    }
    try {
        $options.OutputDirectory = $PSScriptRoot
        & $bench @options
        throw 'Accepted the source tree as output.'
    } catch {
        if ($_.Exception.Message -notlike 'Choose an output directory outside*') { throw }
    }
    Write-Host 'Benchmark reports, comparison, output policy and all three guards passed.'
} finally {
    Remove-Item -LiteralPath $temp -Recurse -Force
}
