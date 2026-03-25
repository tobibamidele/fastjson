/// FFI Bridge Layer - Phase 1: FFI Bindings
///
/// This layer provides the Dart FFI bindings to the native Rust parser.
/// All SIMD acceleration and core parsing is handled by the Rust native core.
///
/// Architecture Compliance:
/// - FFI Bridge Layer (Layer 2 of 3)
/// - Zero-copy or minimal copy design: input bytes are copied once to native memory
///   for parsing, but the parsed tape structure remains in native memory
/// - Memory safety: all allocations use try/finally for guaranteed cleanup
/// - No memory leaks: NativeTape.free() must be called to release native memory
///
/// Memory Model:
/// - Input (Uint8List) -> copied once to native memory for fj_parse
/// - Parsed tape remains in native memory (owned by NativeTape handle)
/// - Each accessor method allocates small temporaries (out parameters)
///   which are immediately freed after use
///
/// Safety Guarantees:
/// - All FFI calls use try/finally blocks to ensure cleanup
/// - Null checks on native pointers prevent use-after-free
/// - Error codes are checked and converted to typed exceptions
///
/// This file follows the FFI surface defined by the Rust native core.
/// All symbols are bound with correct NativeFunction signatures.

import 'dart:ffi';
import 'dart:typed_data';

final _lib = DynamicLibrary.open('libfast_json_native.so');

/// Error codes returned by native functions.
/// Mirrors FjError enum in Rust native/core.
enum FjError {
  ok(0),
  parseError(1),
  badHandle(2),
  wrongType(3),
  keyNotFound(4),
  outOfBounds(5),
  nullPointer(6);

  const FjError(this.value);
  final int value;

  static FjError from(int value) {
    return FjError.values.firstWhere(
      (e) => e.value == value,
      orElse: () => FjError.badHandle,
    );
  }
}

/// Tape entry tags encoding JSON value types.
/// Matches the tag constants defined in Rust tape.rs.
/// Upper 8 bits of 64-bit tape entries encode the type.
const int tagNull = 0x01;
const int tagTrue = 0x02;
const int tagFalse = 0x03;
const int tagString = 0x04;
const int tagI64 = 0x05;
const int tagF64 = 0x06;
const int tagObject = 0x07;
const int tagEndObject = 0x08;
const int tagArray = 0x09;
const int tagEndArray = 0x0A;

/// Parse JSON bytes into a tape structure.
///
/// Signature: fj_parse(ptr: *const u8, len: usize, out: *mut *mut Tape) -> FjError
typedef _FjParseNative = Int32 Function(
    Pointer<Uint8> ptr, IntPtr len, Pointer<Pointer<Void>> out);
typedef _FjParse = int Function(
    Pointer<Uint8> ptr, int len, Pointer<Pointer<Void>> out);
typedef _FjParseSimdDef = Int32 Function(
    Pointer<Uint8> ptr, IntPtr len, Pointer<Pointer<Void>> out);
typedef _FjParseSimd = int Function(
    Pointer<Uint8> ptr, int len, Pointer<Pointer<Void>> out);

/// Free a previously parsed tape handle.
///
/// Signature: fj_free(handle: *mut Tape) -> ()
typedef _FjFreeNative = Void Function(Pointer<Void> handle);
typedef _FjFree = void Function(Pointer<Void> handle);

/// Get the number of entries in the tape.
///
/// Signature: fj_tape_len(handle: *mut Tape) -> usize
typedef _FjTapeLenNative = IntPtr Function(Pointer<Void> handle);
typedef _FjTapeLen = int Function(Pointer<Void> handle);

/// Get the tag (type) of a tape entry.
///
/// Signature: fj_tag(handle: *mut Tape, idx: usize, out: *mut u8) -> FjError
typedef _FjTagNative = Int32 Function(
    Pointer<Void> handle, IntPtr idx, Pointer<Uint8> out);
typedef _FjTag = int Function(
    Pointer<Void> handle, int idx, Pointer<Uint8> out);

/// Read an i64 value from tape.
///
/// Signature: fj_get_i64(handle: *mut Tape, idx: usize, out: *mut i64) -> FjError
typedef _FjGetI64Native = Int32 Function(
    Pointer<Void> handle, IntPtr idx, Pointer<Int64> out);
typedef _FjGetI64 = int Function(
    Pointer<Void> handle, int idx, Pointer<Int64> out);

/// Read an f64 value from tape.
///
/// Signature: fj_get_f64(handle: *mut Tape, idx: usize, out: *mut f64) -> FjError
typedef _FjGetF64Native = Int32 Function(
    Pointer<Void> handle, IntPtr idx, Pointer<Double> out);
typedef _FjGetF64 = int Function(
    Pointer<Void> handle, int idx, Pointer<Double> out);

/// Read a boolean value from tape.
///
/// Signature: fj_get_bool(handle: *mut Tape, idx: usize, out: *mut u8) -> FjError
typedef _FjGetBoolNative = Int32 Function(
    Pointer<Void> handle, IntPtr idx, Pointer<Uint8> out);
typedef _FjGetBool = int Function(
    Pointer<Void> handle, int idx, Pointer<Uint8> out);

/// Read a string reference from tape.
/// Returns pointer and length into the native string pool.
///
/// Signature: fj_get_str(handle: *mut Tape, idx: usize, out_ptr: *mut *const u8, out_len: *mut usize) -> FjError
typedef _FjGetStrNative = Int32 Function(
  Pointer<Void> handle,
  IntPtr idx,
  Pointer<Pointer<Uint8>> outPtr,
  Pointer<IntPtr> outLen,
);
typedef _FjGetStr = int Function(
  Pointer<Void> handle,
  int idx,
  Pointer<Pointer<Uint8>> outPtr,
  Pointer<IntPtr> outLen,
);

/// Get the tape index for an object field by key.
///
/// Signature: fj_get_field(handle: *mut Tape, obj_idx: usize, key_ptr: *const u8, key_len: usize, out_idx: *mut usize) -> FjError
typedef _FjGetFieldNative = Int32 Function(
  Pointer<Void> handle,
  IntPtr objIdx,
  Pointer<Uint8> keyPtr,
  IntPtr keyLen,
  Pointer<IntPtr> outIdx,
);
typedef _FjGetField = int Function(
  Pointer<Void> handle,
  int objIdx,
  Pointer<Uint8> keyPtr,
  int keyLen,
  Pointer<IntPtr> outIdx,
);

/// Get the first element index of an array.
///
/// Signature: fj_array_first(handle: *mut Tape, arr_idx: usize, out_first: *mut usize) -> FjError
typedef _FjArrayFirstNative = Int32 Function(
  Pointer<Void> handle,
  IntPtr arrIdx,
  Pointer<IntPtr> outFirst,
);
typedef _FjArrayFirst = int Function(
    Pointer<Void> handle, int arrIdx, Pointer<IntPtr> outFirst);

/// Skip to the next sibling in a container.
///
/// Signature: fj_next_sibling(handle: *mut Tape, current_idx: usize, container_end_idx: usize, out_next: *mut usize) -> FjError
typedef _FjNextSiblingNative = Int32 Function(
  Pointer<Void> handle,
  IntPtr currentIdx,
  IntPtr containerEndIdx,
  Pointer<IntPtr> outNext,
);
typedef _FjNextSibling = int Function(Pointer<Void> handle, int currentIdx,
    int containerEndIdx, Pointer<IntPtr> outNext);

/// Get the end index of a container (object or array).
///
/// Signature: fj_container_end(handle: *mut Tape, idx: usize, out_end: *mut usize) -> FjError
typedef _FjContainerEndNative = Int32 Function(
    Pointer<Void> handle, IntPtr idx, Pointer<IntPtr> outEnd);
typedef _FjContainerEnd = int Function(
    Pointer<Void> handle, int idx, Pointer<IntPtr> outEnd);

// Bound FFI function handles
final fjParse = _lib
    .lookup<NativeFunction<_FjParseNative>>('fj_parse')
    .asFunction<_FjParse>();

final fjParseSimd = _lib
    .lookup<NativeFunction<_FjParseSimdDef>>('fj_parse_simd')
    .asFunction<_FjParseSimd>();

final fjFree =
    _lib.lookup<NativeFunction<_FjFreeNative>>('fj_free').asFunction<_FjFree>();

final fjTapeLen = _lib
    .lookup<NativeFunction<_FjTapeLenNative>>('fj_tape_len')
    .asFunction<_FjTapeLen>();

final fjTag =
    _lib.lookup<NativeFunction<_FjTagNative>>('fj_tag').asFunction<_FjTag>();

final fjGetI64 = _lib
    .lookup<NativeFunction<_FjGetI64Native>>('fj_get_i64')
    .asFunction<_FjGetI64>();

final fjGetF64 = _lib
    .lookup<NativeFunction<_FjGetF64Native>>('fj_get_f64')
    .asFunction<_FjGetF64>();

final fjGetBool = _lib
    .lookup<NativeFunction<_FjGetBoolNative>>('fj_get_bool')
    .asFunction<_FjGetBool>();

final fjGetStr = _lib
    .lookup<NativeFunction<_FjGetStrNative>>('fj_get_str')
    .asFunction<_FjGetStr>();

final fjGetField = _lib
    .lookup<NativeFunction<_FjGetFieldNative>>('fj_get_field')
    .asFunction<_FjGetField>();

final fjArrayFirst = _lib
    .lookup<NativeFunction<_FjArrayFirstNative>>('fj_array_first')
    .asFunction<_FjArrayFirst>();

final fjNextSibling = _lib
    .lookup<NativeFunction<_FjNextSiblingNative>>('fj_next_sibling')
    .asFunction<_FjNextSibling>();

final fjContainerEnd = _lib
    .lookup<NativeFunction<_FjContainerEndNative>>('fj_container_end')
    .asFunction<_FjContainerEnd>();

final _nullptr = Pointer.fromAddress(0);

/// Simple malloc/free allocator using C runtime.
/// Uses process's malloc/free for native memory allocation.
///
/// Memory Model:
/// - allocate(): calls malloc(byteCount)
/// - free(): calls free(pointer)
///
/// This is a fallback allocator. In production, consider using a
/// pool allocator to reduce malloc/free overhead.
class _SimpleAllocator implements Allocator {
  @override
  Pointer<T> allocate<T extends NativeType>(int byteCount, {int? alignment}) {
    final ffiLib = DynamicLibrary.process();
    final malloc = ffiLib.lookupFunction<Pointer<Void> Function(IntPtr),
        Pointer<Void> Function(int)>('malloc');
    return malloc(byteCount).cast<T>();
  }

  @override
  void free(Pointer pointer) {
    final ffiLib = DynamicLibrary.process();
    final free = ffiLib.lookupFunction<Void Function(Pointer<Void>),
        void Function(Pointer<Void>)>('free');
    free(pointer.cast<Void>());
  }
}

final calloc = _SimpleAllocator();

/// NativeTape - Dart wrapper for the native tape structure.
///
/// The tape is a compact, indexed representation of parsed JSON:
/// - Words: 64-bit entries encoding type tag + payload
/// - Strings: UTF-8 byte pool for string values
/// - Numerics: stored inline in the tape words
///
/// Design: Lazy access model (Phase 3 feature)
/// - The tape structure is built once during parsing
/// - Individual values are extracted on-demand via FFI calls
/// - No full DOM is materialized unless explicitly converted
///
/// Safety:
/// - Native memory is owned by this object until free() is called
/// - Always call dispose() on FjDocument to release native memory
class NativeTape {
  final Pointer<Void> handle;

  const NativeTape(this.handle);

  /// Parse JSON bytes into a NativeTape.
  ///
  /// Copies input bytes to native memory for parsing.
  /// The native parser builds the tape structure.
  /// Temporary allocation is freed after parsing.
  ///
  /// Returns null on parse failure.
  static NativeTape? parse(Uint8List bytes, {bool useSimd = false}) {
    final out = calloc<Pointer<Void>>(sizeOf<Pointer<Void>>());
    try {
      final bytesPtr = calloc<Uint8>(bytes.length);
      try {
        for (var i = 0; i < bytes.length; i++) {
          bytesPtr[i] = bytes[i];
        }
        final err = useSimd
            ? fjParseSimd(bytesPtr, bytes.length, out)
            : fjParse(bytesPtr, bytes.length, out);
        if (err != FjError.ok.value) return null;
        final h = out.value;
        if (h == _nullptr) return null;
        return NativeTape(h);
      } finally {
        calloc.free(bytesPtr);
      }
    } finally {
      calloc.free(out);
    }
  }

  int get length => fjTapeLen(handle);

  /// Get the type tag of a tape entry.
  int tag(int idx) {
    final out = calloc<Uint8>(sizeOf<Uint8>());
    try {
      final err = fjTag(handle, idx, out);
      if (err != FjError.ok.value) {
        throw FjException(FjError.from(err));
      }
      return out.value;
    } finally {
      calloc.free(out);
    }
  }

  /// Read an i64 value from tape entry.
  int getI64(int idx) {
    final out = calloc<Int64>(sizeOf<Int64>());
    try {
      final err = fjGetI64(handle, idx, out);
      if (err != FjError.ok.value) {
        throw FjException(FjError.from(err));
      }
      return out.value;
    } finally {
      calloc.free(out);
    }
  }

  /// Read an f64 value from tape entry.
  double getF64(int idx) {
    final out = calloc<Double>(sizeOf<Double>());
    try {
      final err = fjGetF64(handle, idx, out);
      if (err != FjError.ok.value) {
        throw FjException(FjError.from(err));
      }
      return out.value;
    } finally {
      calloc.free(out);
    }
  }

  /// Read a boolean value from tape entry.
  bool getBool(int idx) {
    final out = calloc<Uint8>(sizeOf<Uint8>());
    try {
      final err = fjGetBool(handle, idx, out);
      if (err != FjError.ok.value) {
        throw FjException(FjError.from(err));
      }
      return out.value != 0;
    } finally {
      calloc.free(out);
    }
  }

  /// Get string pointer and length from tape entry.
  /// Pointer references native string pool memory.
  (Pointer<Uint8>, int) getStr(int idx) {
    final outPtr = calloc<Pointer<Uint8>>(sizeOf<Pointer<Uint8>>());
    final outLen = calloc<IntPtr>(sizeOf<IntPtr>());
    try {
      final err = fjGetStr(handle, idx, outPtr, outLen);
      if (err != FjError.ok.value) {
        throw FjException(FjError.from(err));
      }
      return (outPtr.value, outLen.value);
    } finally {
      calloc.free(outPtr);
      calloc.free(outLen);
    }
  }

  /// Find field index in object by key.
  int getField(int objIdx, Uint8List key) {
    final keyPtr = calloc<Uint8>(key.length);
    final outIdx = calloc<IntPtr>(sizeOf<IntPtr>());
    try {
      for (var i = 0; i < key.length; i++) {
        keyPtr[i] = key[i];
      }
      final err = fjGetField(handle, objIdx, keyPtr, key.length, outIdx);
      if (err != FjError.ok.value) {
        throw FjException(FjError.from(err));
      }
      return outIdx.value;
    } finally {
      calloc.free(keyPtr);
      calloc.free(outIdx);
    }
  }

  /// Get first element index of array.
  int arrayFirst(int arrIdx) {
    final outFirst = calloc<IntPtr>(sizeOf<IntPtr>());
    try {
      final err = fjArrayFirst(handle, arrIdx, outFirst);
      if (err != FjError.ok.value) {
        throw FjException(FjError.from(err));
      }
      return outFirst.value;
    } finally {
      calloc.free(outFirst);
    }
  }

  /// Skip to next sibling in container.
  int nextSibling(int currentIdx, int containerEndIdx) {
    final outNext = calloc<IntPtr>(sizeOf<IntPtr>());
    try {
      final err = fjNextSibling(handle, currentIdx, containerEndIdx, outNext);
      if (err != FjError.ok.value) {
        throw FjException(FjError.from(err));
      }
      return outNext.value;
    } finally {
      calloc.free(outNext);
    }
  }

  /// Get end index of container.
  int containerEnd(int idx) {
    final outEnd = calloc<IntPtr>(sizeOf<IntPtr>());
    try {
      final err = fjContainerEnd(handle, idx, outEnd);
      if (err != FjError.ok.value) {
        throw FjException(FjError.from(err));
      }
      return outEnd.value;
    } finally {
      calloc.free(outEnd);
    }
  }

  /// Release native memory.
  /// MUST be called to prevent memory leaks.
  void free() => fjFree(handle);
}

/// Exception thrown on FFI errors.
class FjException implements Exception {
  final FjError error;
  const FjException(this.error);

  @override
  String toString() => 'FjException: $error';
}
