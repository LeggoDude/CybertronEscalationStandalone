@echo off
rem Builds the demo's game assets (assets\) from your own copy of Transformers: Fall
rem of Cybertron: see tools\extract_game.py and the README's "Build the assets from
rem your game". Run it again after a failure to carry on where it stopped.
cd /d "%~dp0"
where py >nul 2>nul
if %errorlevel%==0 (
  py -3 tools\extract_game.py %*
) else (
  python tools\extract_game.py %*
)
pause
