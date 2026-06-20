Kobayashi self-contained release
=================================

This archive contains everything required to run Kobayashi:

  kobayashi / kobayashi.exe   compiled server
  frontend/dist/              built web interface
  data/                       normalized runtime game data and ability catalogs
  profiles/demo/              bundled starter profile

No repository checkout, Rust toolchain, Node.js installation, or network service is required.

Run
---

macOS / Linux:

  1. Extract the archive.
  2. Open a terminal in the extracted kobayashi-vX.Y.Z folder.
  3. Run: ./kobayashi serve
  4. Open http://localhost:3000

Windows:

  1. Extract the ZIP archive.
  2. Open PowerShell in the extracted kobayashi-vX.Y.Z folder.
  3. Run: .\kobayashi.exe serve
  4. Open http://localhost:3000

The binary finds assets beside itself, so it may also be launched by absolute path or shortcut.
Set KOBAYASHI_HOME to the extracted folder to override asset discovery explicitly.

Player profiles are written under profiles/ in the extracted folder. Keep that directory when
upgrading, or use the profile backup/export controls before replacing an installation.

Verify the SHA-256 digest for your platform archive against SHA256SUMS on the GitHub Release page
before unpacking.

macOS note: the GitHub-hosted macOS build targets Apple Silicon (arm64). On Intel Macs, build from
source with cargo build --release.
