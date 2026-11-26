@echo off
rem Release build of cool_fox with MSVC and the Windows SDK at C:\FakeG\Windows Kits\10.
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
set "SDK=C:\FakeG\Windows Kits\10"
set "SDKV=10.0.26100.0"
set "LIB=%LIB%;%SDK%\Lib\%SDKV%\um\x64;%SDK%\Lib\%SDKV%\ucrt\x64"
set "INCLUDE=%INCLUDE%;%SDK%\Include\%SDKV%\ucrt;%SDK%\Include\%SDKV%\um;%SDK%\Include\%SDKV%\shared"
set "PATH=%SDK%\bin\%SDKV%\x64;%PATH%"
cd /d C:\Users\Vyto\forks\coolfox-pairs
set CARGO_TARGET_DIR=C:\Users\Vyto\forks\coolfox\target-pairs
cargo build --locked --profile diag -p victory --features diagnostics > C:\Users\Vyto\forks\runs\pairs-diag-build.log 2>&1
exit /b %errorlevel%
