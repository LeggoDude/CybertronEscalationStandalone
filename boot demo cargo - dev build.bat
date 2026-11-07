@echo off
title My Automated Tasks
echo Running system diagnostics...

:: --- REPLACE THE LINES BELOW WITH YOUR CMD COMMANDS ---
cd /d "%~dp0"
cargo run
:: -----------------------------------------------------

echo All tasks completed successfully!
pause