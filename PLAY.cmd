@echo off
rem ============================================================
rem  VIctory Fork - One-Click Launcher (root-level)
rem  Everything is in this file: options + explanations + run.
rem  Double-click to start. Edit defaults below if you like.
rem ============================================================
setlocal EnableExtensions
set "ROOT=%~dp0"

rem ---------- defaults ----------
set "VICTORY_GAME_DIR=%ROOT%..\Title.Xbox.v1.0.2.1"
set "RAY_TRACING=1"          rem 1 = title's own ray tracing ON (lit rooms, shadows). 0 = off (faster)
set "DLSS=quality"          rem off | dlaa | quality | balanced | performance
set "FRAME_GEN=0"           rem 0 = off, 2..6 = frames shown per rendered (needs RTX 40+)
set "SERIES_S=0"            rem 0 = Series X, 1 = Series S (faster, softer)
set "RES=1440"              rem 1440 | 1080to1440 | 4k
set "FPS=1000"              rem frame cap (1000 = none; 30 = title's own)

rem ---------- find runner ----------
set "runner=%ROOT%victory.exe"
if not exist "%runner%" set "runner=%ROOT%target\release\victory.exe"
if not exist "%runner%" (
  echo victory.exe not found. Build first: cargo build --release -p victory
  pause
  exit /b 1
)

rem ---------- check game dump ----------
if not exist "%VICTORY_GAME_DIR%\game_scarlett_master.exe" (
  echo Set VICTORY_GAME_DIR to your dump folder containing game_scarlett_master.exe.
  echo Current: %VICTORY_GAME_DIR%
  pause
  exit /b 1
)

rem ============================================================
rem  MENU
rem ============================================================
:menu
cls
echo ============================================================
echo   VIctory Fork - Launcher
echo ============================================================
echo   Game dump : %VICTORY_GAME_DIR%
echo.
echo   1. Ray tracing (shader) : %RAY_TRACING%   (1=ON, 0=OFF)
echo        ON  = title's own ray tracing: lit rooms, lamp/sun shadows.
echo              Slower (~10-13 fps on RTX 4070). Recommended for picture.
echo        OFF = ambient light estimate stands in; faster, no ray-traced
echo              shadows. Use on slower cards or for performance.
echo.
echo   2. DLSS mode            : %DLSS%
echo        off/dlaa/quality/balanced/performance (needs NVIDIA files)
echo.
echo   3. Frame generation     : %FRAME_GEN%   (0=off, 2..6)
echo        Needs RTX 40+ and NVIDIA files.
echo.
echo   4. Console profile      : %SERIES_S%   (0=Series X, 1=Series S)
echo        Series S is faster and softer (1280x720 inside).
echo.
echo   5. Resolution           : %RES%   (1440 | 1080to1440 | 4k)
echo.
echo   6. Frame cap            : %FPS%   (1000=none, 30=title's own)
echo.
echo   R. Run now
echo   Q. Quit
echo ============================================================
choice /c 123456RQ /n /m "Choose: "
if errorlevel 8 goto :end
if errorlevel 7 goto :run
if errorlevel 6 goto :setfps
if errorlevel 5 goto :setres
if errorlevel 4 goto :setseries
if errorlevel 3 goto :setfg
if errorlevel 2 goto :setdlss
if errorlevel 1 goto :setrt
goto :menu

:setrt
set /p RAY_TRACING="Ray tracing 0/1: "
goto :menu
:setdlss
set /p DLSS="DLSS mode: "
goto :menu
:setfg
set /p FRAME_GEN="Frame gen 0/2..6: "
goto :menu
:setseries
set /p SERIES_S="Series S 0/1: "
goto :menu
:setres
set /p RES="Resolution: "
goto :menu
:setfps
set /p FPS="Frame cap: "
goto :menu

:run
rem ---------- build args ----------
set "ARGS=play "%VICTORY_GAME_DIR%\game_scarlett_master.exe""
if "%SERIES_S%"=="1" set "ARGS=%ARGS% --series-s"
if not "%FRAME_GEN%"=="0" set "ARGS=%ARGS% --framegen %FRAME_GEN%"
set "ARGS=%ARGS% --dlss %DLSS% --fps %FPS% --res %RES%"
set "ARGS=%ARGS% --set VICTORY_BVH_BUILDER=%RAY_TRACING%"
set "ARGS=%ARGS% --set VICTORY_AMBIENT=0.8"

rem ---------- launch ----------
"%runner%" %ARGS%
exit /b %errorlevel%

:end
echo Bye.
exit /b 0