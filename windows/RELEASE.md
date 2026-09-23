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
- **Auto-update signing key** (W-10, built): the key pair was generated on the development PC.
  The public key is in `codenotch/tauri.conf.json`; the private key is at
  `%USERPROFILE%\.tauri\codenotch-updater.key` and is **not** in the repo. Add its full contents as the
  repository secret `TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, empty unless you
  re-encrypt it). Back the file up somewhere safe: losing it means existing installs can never be updated
  again; leaking it means anyone can push an update to them.

## How auto-update works
Every `win-v*` tag builds a signed installer plus `latest.json` and attaches both to the GitHub release.
Installed copies read `releases/latest/download/latest.json` every 6 h (settings can turn this off), verify the
signature against the shipped public key, and offer "Install update" in the tray and settings. Nothing is
installed without a click. The version in the tag must be higher than the installed one, so bump all three
version fields before tagging. A local `scripts\build-installer.ps1` also signs when the key file is present.
