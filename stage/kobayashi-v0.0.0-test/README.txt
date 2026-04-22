Kobayashi prebuilt bundle (binary + web UI)
============================================

This archive contains only the compiled `kobayashi` server binary and the built SPA
under `frontend/dist/`. Game data (`data/`), profiles, and other files still come from
a checkout of this repository at the same release tag.

Recommended setup
-----------------

1. Clone the repository and check out the matching tag (example: v0.1.0).
2. Extract this archive at the repository root so you have:
     ./kobayashi              (Windows: kobayashi.exe in this folder)
     ./frontend/dist/         (replaces or adds the built UI)
     ./data/                  (already from the clone)
     ./profiles/              (optional; create or import your profile)
3. From the repository root, run:
     ./kobayashi serve
   Then open http://localhost:3000 (or the host/port set in KOBAYASHI_BIND).

Verify the SHA-256 digest for your platform’s archive against `SHA256SUMS` on the
GitHub Release page before unpacking.

macOS note: the GitHub-hosted macOS build targets Apple Silicon (arm64). On Intel Macs,
build from source with `cargo build --release` instead.
