BoilerSteamXL — portable edition
================================

1. Extract this folder anywhere you can write to (for example D:\Tools\BoilerSteamXL).
2. Start BoilerSteamXL.exe. No installation and no administrator rights are needed.
3. Your settings, review queue, logs and backups live in the BoilerSteamXL-data folder
   created next to the executable.

Requirements: Windows 10/11 x64 with the Microsoft Edge WebView2 Runtime (built into
Windows 11). The app is not code-signed, so Windows SmartScreen may ask you to confirm the
first start ("More info" → "Run anyway").

Safety: BoilerSteamXL starts in Simulation mode. It reads Steam but never writes to it
until you switch Simulation mode off in Settings → Safety. Every import then still needs
your approval, a final confirmation and a verified backup.

Scheduled refresh: if you turn on a daily/weekly refresh in Automation, Windows Task
Scheduler runs this executable with --scan. If you move the folder, open the app and save
the schedule again so the task points to the new location.

Command line:
  BoilerSteamXL.exe --scan      look for new games (never imports)
  BoilerSteamXL.exe --sync      --scan, then import games you already confirmed
  BoilerSteamXL.exe --dry-run   show what importing approved games would do
  BoilerSteamXL.exe --status    review queue, last scan, schedule

BoilerSteamXL is based on BoilR (https://github.com/PhilipK/BoilR) by Philip Kristoffersen
and contributors, licensed MIT OR Apache-2.0. It is not affiliated with Valve or the BoilR
project.
