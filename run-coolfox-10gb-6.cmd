@echo off
rem codec-d/cool_fox (f4b48ad), its own Play-VIctory-pairs.cmd pareto-residency settings unchanged,
rem only the game folder, a log file and the frame log added.
set "VICTORY_GAME_DIR=C:\Users\Vyto\Downloads\GTA6"
rem 10 GB TEST 2 (one change against cool_fox's own settings; test 1's CARD_WINDOWS_MB=1792 dropped: worse, this fork reads no windows).
rem Card copies of the title's buffers: this fork caps them at 512 MB on cards of 12 GB or less; v11 ran well with 1024.
set VICTORY_CARD_BUFFERS_MB=1024
rem 10 GB TEST 6 (one change against test 2, the failsafe): the build with soldsawn's exact wave64 paired form ported
rem (branch wave64-pairs, 60718b6, built in target-pairs): every 64-lane shader translates (26 -> 0 refused).
rem Records every pipeline the title makes, so victory warm --used can build them before later runs.
set VICTORY_PIPELINE_BOOK=1
set VICTORY_FRAME_LOG=1
set RUST_LOG=info
if not exist "C:\Users\Vyto\forks\runs" mkdir "C:\Users\Vyto\forks\runs"
call "C:\Users\Vyto\forks\coolfox\Play-VIctory-pairs.cmd" pareto-residency > "C:\Users\Vyto\forks\runs\coolfox-10gb-6.log" 2>&1
