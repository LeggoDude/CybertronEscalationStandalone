@echo off
title My Automated Tasks
echo Running system diagnostics...

:: --- REPLACE THE LINES BELOW WITH YOUR CMD COMMANDS ---
cd /d "%~dp0"
echo Building and starting the optimized demo (the first build takes several minutes)...
cargo run --release
:: -----------------------------------------------------

echo All tasks completed successfully!
pause