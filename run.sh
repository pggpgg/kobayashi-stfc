#!/usr/bin/env bash
# KOBAYASHI — Build, serve, and open the web interface
# Usage: ./run.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "⚔ KOBAYASHI — Build & Run"
echo ""

# Step 1: Build Rust binary
echo "[1/4] Building Rust binary..."
cargo build --release
echo ""

# Step 2: Build frontend
echo "[2/4] Building frontend..."
cd frontend
npm install --silent 2>/dev/null || npm install
npm run build
cd ..
echo ""

# Step 3: Detect executable (Windows vs Unix)
if [[ -f target/release/kobayashi.exe ]]; then
  KOBAYASHI="./target/release/kobayashi.exe"
else
  KOBAYASHI="./target/release/kobayashi"
fi

# Step 4: Start server in background and open browser
echo "[3/4] Starting server..."
$KOBAYASHI serve &
SERVER_PID=$!

# Give the server a moment to bind
sleep 2

echo "[4/4] Opening http://localhost:3000 ..."
if command -v xdg-open >/dev/null 2>&1; then
  xdg-open http://localhost:3000
elif command -v open >/dev/null 2>&1; then
  open http://localhost:3000
elif command -v start >/dev/null 2>&1; then
  start http://localhost:3000
else
  echo "Could not detect browser launcher. Open http://localhost:3000 manually."
fi

echo ""
echo "✅ KOBAYASHI is running. Press Ctrl+C to stop the server."
echo ""

# Wait for server; Ctrl+C will kill both script and server
wait $SERVER_PID
