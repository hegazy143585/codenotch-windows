# Builds the Windows installer (NSIS .exe) locally. Run from the `windows` folder:
#   powershell -ExecutionPolicy Bypass -File scripts\build-installer.ps1
# Needs: Rust (MSVC toolchain) and tauri-cli 2 (`cargo install tauri-cli --version "^2" --locked`).
$ErrorActionPreference = "Stop"
$triple = (rustc -vV | Select-String "^host:").ToString().Split(" ")[1]

cargo test --workspace --locked
cargo build --release --locked -p codenotch-hook

# Tauri sidecar naming: the file carries the target triple; the installer drops the suffix,
# so codenotch-hook.exe lands next to codenotch.exe where hooks_install.rs looks for it.
New-Item -ItemType Directory -Force codenotch\binaries | Out-Null
Copy-Item target\release\codenotch-hook.exe "codenotch\binaries\codenotch-hook-$triple.exe" -Force

Push-Location codenotch
cargo tauri build --config tauri.release.conf.json
Pop-Location

Get-ChildItem target\release\bundle\nsis\*.exe | ForEach-Object { Write-Host "Installer: $($_.FullName)" }
