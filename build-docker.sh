#!/bin/bash
#
# Build Hortus dans un container Docker Ubuntu 22.04
# Compatible avec les VPS Ubuntu 22.04 (GLIBC 2.35)
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
cd "$SCRIPT_DIR"

IMAGE_NAME="hortus-builder"

echo "=== Build Hortus (Docker Ubuntu 22.04) ==="

# Etape 1: Construire l'image Docker (si necessaire)
if ! docker images | grep -q "$IMAGE_NAME"; then
    echo "[1/2] Construction de l'image Docker (premiere fois, ~5 min)..."
    docker build --network host -f Dockerfile.build -t "$IMAGE_NAME" .
else
    echo "[1/2] Image Docker existante, skip..."
fi

# Etape 2: Compiler le backend Rust dans le container (depuis backend/)
echo "[2/2] Compilation du backend Rust (release)..."

mkdir -p backend/target-docker

docker run --rm \
    --network host \
    -e CARGO_TARGET_DIR=/app/backend/target-docker \
    -e CARGO_HOME=/root/.cargo \
    -v "$SCRIPT_DIR:/app" \
    -v cargo-cache-hortus:/root/.cargo/registry \
    -w /app/backend \
    "$IMAGE_NAME" \
    bash -c "cargo build --release --bin hortus-backend 2>&1 | tail -50"

# Verifier que le binaire existe
if [ ! -f "backend/target-docker/release/hortus-backend" ]; then
    echo "ERREUR: Le binaire n'a pas ete cree"
    exit 1
fi

# Copier vers backend/target/release pour compatibilite avec deploy.sh
mkdir -p backend/target/release
cp backend/target-docker/release/hortus-backend backend/target/release/

echo "[OK] Backend compile: $(du -h backend/target/release/hortus-backend | cut -f1)"
echo ""
echo "=== Build termine ==="
echo "Binaire: backend/target/release/hortus-backend"
echo ""
echo "Pour deployer: ./deploy.sh deploy"
