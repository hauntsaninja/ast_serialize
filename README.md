# ast_serialize - Python AST Serializer

Python extension for parsing Python files and serializing their AST to mypy's binary format.

## Installation

```bash
pip install ast_serialize
```

## Usage

```python
import ast_serialize

# Parse a Python file and get serialized AST bytes
result = ast_serialize.parse("myfile.py")
print(f"Serialized AST: {len(result)} bytes")
```

## Building

See [BUILD.md](BUILD.md) for detailed build instructions.

### Quick Build Commands

```bash
# Build all wheels (stable ABI + free-threaded)
./build_wheels.sh

# Or manually:
# Stable ABI wheel (Python 3.9+)
maturin build --release

# Free-threaded wheel (Python 3.13t)
maturin build --release --no-default-features --features free-threaded --interpreter python3.13t
```

Wheels are placed in: `/home/jukka/src/ruff/target/wheels/`

## Wheel Types

| Wheel Type | Filename Pattern | Compatible With |
|------------|------------------|-----------------|
| Stable ABI | `cp39-abi3-*.whl` | Python 3.9, 3.10, 3.11, 3.12, 3.13+ (GIL) |
| Free-threaded | `cp313t-cp313t-*.whl` | Python 3.13t (nogil) only |

## Development

```bash
# Development build (fast compilation, unoptimized)
maturin develop

# Run tests
python3 test_ast_serialize.py
```

## License

See repository license
