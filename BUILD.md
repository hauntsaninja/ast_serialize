# Building ast_serialize Wheels

This document explains how to build Python wheels for the `ast_serialize` extension.

## Quick Start

### Build All Wheels (Recommended)

```bash
./build_wheels.sh
```

This builds:
- **Stable ABI wheel** (`cp39-abi3`): Works on Python 3.9, 3.10, 3.11, 3.12, 3.13+
- **Free-threaded wheel** (`cp313t`): For nogil Python 3.13t (if installed)

## Manual Building

### 1. Stable ABI Wheel (Python 3.9+)

Build a single wheel that works across multiple Python versions:

```bash
maturin build --release
# or explicitly:
maturin build --release --features stable-abi
```

**Output:** `ast_serialize-0.1.0-cp39-abi3-manylinux_*.whl`

**Compatible with:** Python 3.9, 3.10, 3.11, 3.12, 3.13, and future versions

### 2. Free-Threaded Wheel (Python 3.13t)

Build for free-threaded (nogil) Python:

```bash
maturin build --release --no-default-features --features free-threaded --interpreter python3.13t
```

**Output:** `ast_serialize-0.1.0-cp313t-cp313t-manylinux_*.whl`

**Compatible with:** Only Python 3.13t (free-threaded build)

**Note:** Requires Python 3.13t to be installed. Get it from:
- https://www.python.org/downloads/ (experimental build)
- Or build from source with `--disable-gil` flag

### 3. Development Build

For testing during development (unoptimized, fast compilation):

```bash
maturin develop
# or for free-threaded:
maturin develop --no-default-features --features free-threaded --interpreter python3.13t
```

## Wheel Locations

All wheels are placed in:
```
/home/jukka/src/ruff/target/wheels/
```

## Distribution

### PyPI Upload

```bash
# Upload stable ABI wheel (recommended for most users)
twine upload target/wheels/ast_serialize-*-cp39-abi3-*.whl

# Optionally upload free-threaded wheel for early adopters
twine upload target/wheels/ast_serialize-*-cp313t-*.whl
```

### Local Installation

```bash
pip install target/wheels/ast_serialize-*.whl
```

## Architecture Notes

### Stable ABI vs Free-Threaded

- **Stable ABI** and **Free-Threaded** are **mutually exclusive**
- You must build separate wheels for each
- Most users should use the stable ABI wheel
- Free-threaded wheel is for users who have nogil Python 3.13t

### Feature Flags

Defined in `Cargo.toml`:

| Feature | Description | Default |
|---------|-------------|---------|
| `stable-abi` | Enable Python 3.9+ stable ABI | ✅ Yes |
| `free-threaded` | Marker for free-threaded builds | ❌ No |

## Troubleshooting

### "python3.13t: command not found"

You don't have free-threaded Python installed. Either:
- Install Python 3.13t from python.org
- Skip free-threaded builds (stable ABI wheel works on regular Python 3.13)

### "abi3 and free-threaded are incompatible"

This is expected. Make sure you use `--no-default-features` when building free-threaded wheels to disable the default `stable-abi` feature.

## CI/CD Example

```yaml
- name: Build wheels
  run: |
    # Stable ABI wheel (works on most Python versions)
    maturin build --release --features stable-abi

    # Free-threaded wheel (if Python 3.13t available)
    if command -v python3.13t; then
      maturin build --release --no-default-features --features free-threaded --interpreter python3.13t
    fi
```
