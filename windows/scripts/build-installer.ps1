# Builds the Windows installer (NSIS .exe) locally. Run from the `windows` folder:
#   powershell -ExecutionPolicy Bypass -File scripts\build-installer.ps1
# Needs: Rust (MSVC toolchain) and tauri-cli 2 (`cargo install tauri-cli --version "^2" --locked`).
# If the updater key exists at %USERPROFILE%\.tauri\codenotch-updater.key, the installer is also
# signed for auto-update (a .sig next to the .exe). The key is read from disk, never printed.
$ErrorActionPreference = "Stop"
$triple = (rustc -vV | Select-String "^host:").ToString().Split(" ")[1]

cargo test --workspace --locked
cargo build --release --locked -p codenotch-hook

# Tauri sidecar naming: the file carries the target triple; the installer drops the suffix,
# so codenotch-hook.exe lands next to codenotch.exe where hooks_install.rs looks for it.
New-Item -ItemType Directory -Force codenotch\binaries | Out-Null
Copy-Item target\release\codenotch-hook.exe "codenotch\binaries\codenotch-hook-$triple.exe" -Force

$updaterArgs = @()
$key = Join-Path $env:USERPROFILE ".tauri\codenotch-updater.key"
if (Test-Path $key) {
    $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $key -Raw).Trim()
    # The key is password-protected: Windows cannot hold an empty environment variable, and an unset
    # password makes the signer wait for a prompt nobody sees. The password file sits next to the key.
    $pw = Join-Path $env:USERPROFILE ".tauri\codenotch-updater.password"
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
        if (-not (Test-Path $pw)) { throw "Updater key found but no $pw - set TAURI_SIGNING_PRIVATE_KEY_PASSWORD" }
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (Get-Content $pw -Raw).Trim()
    }
    $updaterArgs = @("--config", "tauri.updater.conf.json")
    Write-Host "Updater key found: the installer will be signed for auto-update"
} else {
    Write-Host "No updater key: building without auto-update artifacts"
}

Push-Location codenotch
cargo tauri build --config tauri.release.conf.json @updaterArgs
Pop-Location

Get-ChildItem target\release\bundle\nsis\*.exe, target\release\bundle\nsis\*.sig -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "Output: $($_.FullName)" }
