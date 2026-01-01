#!/usr/bin/env bash
# Build script for creating both stable ABI and free-threaded wheels

set -e  # Exit on error

echo "======================================"
echo "Building Python wheels for ast_serialize"
echo "======================================"
echo ""

# Build stable ABI wheel for Python 3.9+
echo "📦 Building stable ABI wheel (Python 3.9+)..."
maturin build --release --features stable-abi
echo "✓ Stable ABI wheel complete"
echo ""

# Build free-threaded wheel for Python 3.13t (if available)
echo "📦 Building free-threaded wheel (Python 3.13t)..."
if command -v python3.13t &> /dev/null; then
    maturin build --release --no-default-features --features free-threaded --interpreter python3.13t
    echo "✓ Free-threaded wheel complete"
else
    echo "⚠️  Python 3.13t not found - skipping free-threaded build"
    echo "   Install free-threaded Python to build this variant"
fi
echo ""

# List all built wheels
echo "======================================"
echo "Built wheels:"
echo "======================================"
ls -lh ../../target/wheels/ast_serialize-*.whl 2>/dev/null || echo "No wheels found in target/wheels/"
echo ""
echo "Wheels location: $(pwd)/../../target/wheels/"
