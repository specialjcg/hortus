#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$ROOT/backend"
FRONTEND_DIR="$ROOT/frontend"
BACKEND_PORT="${BACKEND_PORT:-3000}"
FRONTEND_PORT="${FRONTEND_PORT:-8000}"

cleanup() {
    trap - INT TERM EXIT
    echo
    echo "[stop] arrêt..."
    [[ -n "${BACKEND_PID:-}" ]]  && kill "$BACKEND_PID"  2>/dev/null || true
    [[ -n "${FRONTEND_PID:-}" ]] && kill "$FRONTEND_PID" 2>/dev/null || true
    pkill -P $$ 2>/dev/null || true
    exit 0
}
trap cleanup INT TERM EXIT

check_port_free() {
    local port="$1" name="$2"
    if ss -tln 2>/dev/null | awk '{print $4}' | grep -qE ":${port}$"; then
        echo "[error] port $port ($name) occupé." >&2
        echo "        inspecter: ss -tlnp | grep :$port" >&2
        exit 1
    fi
}

echo "==== Hortus dev ===="
echo

check_port_free "$BACKEND_PORT" backend
check_port_free "$FRONTEND_PORT" frontend

echo "[build] backend (release)..."
(cd "$BACKEND_DIR" && cargo build --release --bin hortus-backend)

echo "[build] frontend..."
if [[ ! -x "$FRONTEND_DIR/node_modules/.bin/elm" ]]; then
    echo "[build]   installation des devDependencies npm..."
    (cd "$FRONTEND_DIR" && npm install --silent)
fi
(cd "$FRONTEND_DIR" && ./node_modules/.bin/elm make src/Main.elm --output=static/elm.js)

echo
echo "[run] backend  -> http://localhost:$BACKEND_PORT"
RUST_LOG="${RUST_LOG:-info}" PORT="$BACKEND_PORT" "$BACKEND_DIR/target/release/hortus-backend" &
BACKEND_PID=$!

echo "[run] frontend -> http://localhost:$FRONTEND_PORT"
(cd "$FRONTEND_DIR/static" && python3 -m http.server "$FRONTEND_PORT") &
FRONTEND_PID=$!

echo
echo "==== prêt ===="
echo "  backend   http://localhost:$BACKEND_PORT"
echo "  frontend  http://localhost:$FRONTEND_PORT"
echo "  ctrl-c    pour arrêter"
echo

wait
