# FastJson
**Note:** This is a learning project, I was fascinated by the performance benefits SIMD was able to gain while processing large json inputs in python and go's sonic, so I thought to myself why not just make one for Dart. It did cross my mind to write a pure Dart SIMD JSON parser but we're all about the performance benefits here :) \
Nevertheless, it was a fun project I worked on with Claude and although I might say this isn't ready for prod, you may find a way to work around it. Thanks.

A high-performance JSON parser for Dart, powered by Rust with SIMD acceleration.

## Overview

FastJson provides a significantly faster JSON parsing alternative to `dart:convert`, using a Rust native core with optional SIMD acceleration and a minimal-allocation FFI bridge.

**Performance Target:** 3-6x faster parsing vs `dart:convert` on large payloads.

## Architecture

The system is designed in three layers:

```
Layer 1: Native Core (Rust)
  - SIMD-accelerated scanning
  - Tape-based document representation
  - UTF-8 validation

Layer 2: FFI Bridge
  - Zero-copy or minimal-copy design
  - dart:ffi bindings
  - Manual memory management

Layer 3: Dart API
  - Lazy DOM-style access
  - Typed decoding support
  - Codegen hooks (Phase 4)
```

## File Structure

```
fast_json/
  dart/
    fast_json.dart          # Barrel export
    pubspec.yaml            # Dependencies
    src/
      ffi.dart             # FFI bindings
      models.dart          # FjValue, FjArray
      api.dart             # FastJson facade
      decoder.dart         # Typed decoding
    benchmark/
      bench.dart           # Performance benchmarks
  native/
    rust/
      src/
        lib.rs             # FFI exports
        parser.rs          # Recursive descent parser
        simd_scan.rs       # Structural scanning
        tape.rs            # Tape representation
        decode.rs          # Value decoding
```

## Performance benefits
When run on my dusty, old, crony laptop, I get around a 5x performance benefit over traditional `dart:convert`.

- On a 1MB JSON object
```bash
=== 1 MB Payload Benchmark ===
FastJson: 96ms total, 213.76 MB/s
dart:convert: 546ms total, 37.58 MB/s
```

- On a 10MB JSON object
```bash
=== 10 MB Payload Benchmark ===
FastJson: 1347ms total, 155.31 MB/s
dart:convert: 7311ms total, 28.61 MB/s
```

- On a 50MB JSON object
```bash
=== 50 MB Payload Benchmark ===
FastJson: 39505ms total, 26.72 MB/s
dart:convert: 215988ms total, 4.89 MB/s
```

## Usage

### Basic Parsing

```dart
import 'package:fast_json/fast_json.dart';

final bytes = Uint8List.fromList('{"key": "value"}'.codeUnits);
final doc = FastJson.parse(bytes);

if (doc != null) {
  try {
    final value = doc['key'].asString();
    print(value); // "value"
  } finally {
    doc.dispose();
  }
}
```

### Lazy Access

```dart
final doc = FastJson.parse(bytes);

// Access nested fields without materializing full document
final userName = doc['user']['profile']['name'].asString();

// Iterate arrays lazily
for (final item in doc['items'].asArray()) {
  process(item.asString());
}
```

### Typed Decoding

```dart
// Register adapters
class User {
  final String name;
  final int age;
  User(this.name, this.age);
}

FjTypeRegistry.register<User>((v, ctx) {
  return User(
    name: v.getField('name').asString(),
    age: v.getField('age').asInt(),
  );
});

final doc = FastJson.parse(bytes);
final user = docDecode<User>(doc);
doc.dispose();
```

### Materializing Full DOM

```dart
final doc = FastJson.parse(bytes);

// Convert to standard Dart Map/List
final Map<String, dynamic> data = doc.toJson();
doc.dispose();
```

## Building

### Prerequisites

- Rust toolchain (stable)
- Dart SDK 3.0+

### Build Native Library

```bash
cd native/rust
cargo build --release
```

The shared library will be at `target/release/libfast_json_native.so`.

### Setup Dart Dependencies

```bash
cd dart
dart pub get
```

### Run Benchmarks

```bash
cd dart
LD_LIBRARY_PATH=../native/rust/target/release:$LD_LIBRARY_PATH \
  dart run benchmark/bench.dart --dump
```

The `--dump` flag writes the test JSON to files for inspection.

## Performance Characteristics

| Feature | Implementation |
|---------|----------------|
| SIMD Scanning | Phase 2 (planned) |
| Tape Representation | Implemented |
| Lazy Access | Implemented |
| Typed Decoding | Registry + adapters |
| Codegen | Phase 4 (planned) |
| Memory Model | Manual dispose() |

### Phase Status

| Phase | Status |
|-------|--------|
| Phase 1: Core Parser + FFI | Complete |
| Phase 2: SIMD Optimization | Planned |
| Phase 3: Lazy Access | Implemented |
| Phase 4: Codegen | Planned |

## API Reference

### FastJson

- `FastJson.parse(Uint8List bytes)` -> `FjDocument?`
- `FastJson.parseString(String json)` -> `FjDocument?`

### FjDocument

- `dispose()` - Release native memory
- Inherited from FjValue

### FjValue

- `asString()` -> `String`
- `asInt()` -> `int`
- `asDouble()` -> `double`
- `asBool()` -> `bool`
- `asArray()` -> `FjArray`
- `getField(String key)` -> `FjValue`
- `toJson()` -> `dynamic`
- Type checks: `isNull`, `isBool`, `isInt`, `isDouble`, `isString`, `isObject`, `isArray`

### FjArray

- `length` -> `int`
- `at(int index)` -> `FjValue`
- `iterable` -> `Iterable<FjValue>`
- `toList()` -> `List<dynamic>`

## Limitations

- Manual memory management required (no finalizers yet)
- String access copies from native memory
- Windows support requires library name adjustment

## License

MIT
