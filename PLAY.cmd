@echo off
rem One-click launcher for the VIctory fork. Double-click to start.
rem Requires victory.exe (built) beside this file or in target\release, and
rem a dump folder with game_scarlett_master.exe (set VICTORY_GAME_DIR).
setlocal
set "runner=%~dp0victory.exe"
if not exist "%runner%" set "runner=%~dp0target\release\victory.exe"
if not exist "%runner%" (
  echo victory.exe not found. Build first: cargo build --release -p victory
  pause
  exit /b 1
)
if not defined VICTORY_GAME_DIR set "VICTORY_GAME_DIR=%~dp0..\Title.Xbox.v1.0.2.1"
if not exist "%VICTORY_GAME_DIR%\game_scarlett_master.exe" (
  echo Set VICTORY_GAME_DIR to your dump folder containing game_scarlett_master.exe.
  pause
  exit /b 1
)
"%runner%" play "%VICTORY_GAME_DIR%\game_scarlett_master.exe"
exit /b %errorlevel%