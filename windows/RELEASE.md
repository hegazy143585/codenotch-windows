# Releasing Codenotch for Windows

## Automatic (GitHub Actions)
1. Bump `version` in `windows/codenotch/Cargo.toml`, `windows/codenotch-hook/Cargo.toml`, and `windows/codenotch/tauri.conf.json`.
2. Commit and push to `main`. The **Windows** workflow runs tests and builds the installer. Download it from the run page (artifact `codenotch-windows-<sha>`).
3. To publish: `git tag win-v0.4.0 && git push origin win-v0.4.0`. The workflow attaches the installer to a GitHub release.

## Local build (Windows)
Needs Rust (MSVC toolchain) and `cargo install tauri-cli --version "^2" --locked`.
```powershell
cd windows
powershell -ExecutionPolicy Bypass -File scripts\build-installer.ps1
```
Output: `windows\target\release\bundle\nsis\Codenotch_<version>_x64-setup.exe`.
The installer is per-user (no admin prompt), adds a Start Menu entry and an uninstaller, and installs
`codenotch-hook.exe` next to `codenotch.exe`.

## Still needs credentials from you
- **Code signing** (removes the SmartScreen warning): an OV/EV certificate or Azure Trusted Signing.
  Add it as repository secrets, then set `bundle.windows.signCommand` (Trusted Signing) or
  `certificateThumbprint` in `tauri.release.conf.json`.
- **Auto-update** (W-10, not built yet): a Tauri updater key pair; the private key goes in repository secrets.
