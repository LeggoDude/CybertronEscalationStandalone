@echo off
rem Makes the installer for players, installer_out\Cybertron Escalation Setup (version).exe:
rem see tools\make_installer.py and the README's "Making the installer for players". It
rem needs Inno Setup 6 (https://jrsoftware.org/isdl.php) besides what the demo's own build
rem and "extract assets.bat" need.
cd /d "%~dp0"
where py >nul 2>nul
if %errorlevel%==0 (
  py -3 tools\make_installer.py %*
) else (
  python tools\make_installer.py %*
)
pause
