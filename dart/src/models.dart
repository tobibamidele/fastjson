/// Models Layer - Phase 1: Lazy DOM-style Access
///
/// This layer provides the Dart-side value types for navigating parsed JSON.
/// Designed for lazy evaluation: no full DOM is materialized unless requested.
///
/// Architecture Compliance:
/// - Dart API Layer (Layer 3 of 3)
/// - Lazy evaluation: values are extracted from tape on-demand
/// - Zero-copy strings: strings reference native memory, copied only when accessed
/// - Type-safe accessors with automatic coercion where sensible
///
/// Design Philosophy:
/// - FjValue is a view into the native tape, not a copy
/// - Accessors (asString, asInt, etc.) copy data only when needed
/// - toJson() materializes full DOM if user needs Map/List representation
///
/// Performance Notes:
/// - Single FFI call per value access (tag lookup is cached per FjValue)
/// - Array iteration uses iterator pattern to avoid index-based seeking
/// - String access copies from native pool (unavoidable for Dart String)
///
/// Memory Safety:
/// - FjValue holds references to NativeTape (owned by FjDocument)
/// - FjDocument.dispose() must be called to release native memory
/// - No Dart finalizers yet - manual lifecycle management required

import 'dart:convert';
import 'dart:ffi';
import 'dart:typed_data';

import 'ffi.dart';

/// FjValue - A view into a position in the native tape.
///
/// This is a lazy reference to a JSON value. It does not copy data
/// until an accessor method is called.
///
/// Example:
/// ```dart
/// final doc = FastJson.parse(bytes);
/// final name = doc["user"]["name"].asString();  // Lazy access
/// final all = doc.toJson();                      // Materialize full DOM
/// doc.dispose();
/// ```
class FjValue {
  /// Reference to the native tape structure.
  final NativeTape tape;

  /// Index into the tape for this value.
  final int tapeIdx;

  const FjValue(this.tape, this.tapeIdx);

  /// Get the type tag of this value.
  int get tag => tape.tag(tapeIdx);

  /// Type checks for JSON value types.
  bool get isNull => tag == tagNull;
  bool get isBool => tag == tagTrue || tag == tagFalse;
  bool get isInt => tag == tagI64;
  bool get isDouble => tag == tagF64;
  bool get isString => tag == tagString;
  bool get isObject => tag == tagObject;
  bool get isArray => tag == tagArray;
  bool get isTrue => tag == tagTrue;
  bool get isFalse => tag == tagFalse;

  /// Extract string value, copying from native memory.
  ///
  /// Note: This copies the string bytes and decodes UTF-8.
  /// Zero-copy is not possible with Dart String (immutable).
  String asString() {
    if (tag != tagString) {
      throw FjException(FjError.wrongType);
    }
    final (ptr, len) = tape.getStr(tapeIdx);
    if (ptr == Pointer.fromAddress(0) || len == 0) return '';
    final bytes = Uint8List(len);
    for (var i = 0; i < len; i++) {
      bytes[i] = ptr[i];
    }
    return utf8.decode(bytes);
  }

  /// Extract integer value.
  ///
  /// Type coercion:
  /// - tagI64: direct read
  /// - tagF64: truncate to int
  /// - tagTrue/tagFalse: 1/0
  /// - tagString: parse integer
  int asInt() {
    if (tag == tagI64) {
      return tape.getI64(tapeIdx);
    }
    if (tag == tagF64) {
      return tape.getF64(tapeIdx).toInt();
    }
    if (tag == tagTrue) return 1;
    if (tag == tagFalse) return 0;
    if (tag == tagString) {
      return int.tryParse(asString()) ?? 0;
    }
    throw FjException(FjError.wrongType);
  }

  /// Extract boolean value.
  ///
  /// Type coercion:
  /// - tagTrue/tagFalse: direct
  /// - tagI64/tagF64: != 0
  /// - tagString: "true"/"1"
  bool asBool() {
    if (tag == tagTrue) return true;
    if (tag == tagFalse) return false;
    if (tag == tagI64) return tape.getI64(tapeIdx) != 0;
    if (tag == tagF64) return tape.getF64(tapeIdx) != 0;
    if (tag == tagString) {
      final s = asString().toLowerCase();
      return s == 'true' || s == '1';
    }
    throw FjException(FjError.wrongType);
  }

  /// Extract double value.
  ///
  /// Type coercion:
  /// - tagF64: direct read
  /// - tagI64: convert to double
  /// - tagString: parse double
  double asDouble() {
    if (tag == tagF64) {
      return tape.getF64(tapeIdx);
    }
    if (tag == tagI64) {
      return tape.getI64(tapeIdx).toDouble();
    }
    if (tag == tagString) {
      return double.tryParse(asString()) ?? 0.0;
    }
    throw FjException(FjError.wrongType);
  }

  /// Get the number of children in a container.
  ///
  /// For objects: number of key-value pairs
  /// For arrays: number of elements
  /// For strings: byte length
  int get length {
    if (tag == tagObject || tag == tagArray) {
      final end = tape.containerEnd(tapeIdx);
      int count = 0;
      var idx = tapeIdx + 1;
      while (idx < end) {
        count++;
        idx = tape.nextSibling(idx, end);
      }
      return count;
    }
    if (tag == tagString) {
      final (_, len) = tape.getStr(tapeIdx);
      return len;
    }
    throw FjException(FjError.wrongType);
  }

  /// Get a field value from an object.
  ///
  /// This is the lazy access pattern for selective parsing.
  /// Only the requested field is accessed, not the entire object.
  ///
  /// Example:
  /// ```dart
  /// final value = doc["user"]["profile"]["name"].asString();
  /// ```
  FjValue getField(String key) {
    if (tag != tagObject) {
      throw FjException(FjError.wrongType);
    }
    final keyBytes = utf8.encode(key);
    final fieldIdx = tape.getField(tapeIdx, keyBytes);
    return FjValue(tape, fieldIdx);
  }

  /// Get array view for iteration.
  FjArray asArray() {
    if (tag != tagArray) {
      throw FjException(FjError.wrongType);
    }
    return FjArray(tape, tapeIdx);
  }

  /// Materialize the full value as a Dart object.
  ///
  /// This converts the lazy view into a materialized DOM:
  /// - null/true/false: same
  /// - numbers: int or double
  /// - strings: String
  /// - objects: Map<String, dynamic>
  /// - arrays: List<dynamic>
  ///
  /// Warning: This recursively materializes the entire subtree.
  /// For large JSON, prefer lazy access pattern.
  dynamic toJson() {
    switch (tag) {
      case tagNull:
        return null;
      case tagTrue:
        return true;
      case tagFalse:
        return false;
      case tagI64:
        return asInt();
      case tagF64:
        return asDouble();
      case tagString:
        return asString();
      case tagObject:
        return _toJsonObject();
      case tagArray:
        return _toJsonArray();
      default:
        throw FjException(FjError.badHandle);
    }
  }

  Map<String, dynamic> _toJsonObject() {
    final result = <String, dynamic>{};
    final end = tape.containerEnd(tapeIdx);
    var idx = tapeIdx + 1;
    while (idx < end) {
      final key = FjValue(tape, idx).asString();
      idx = tape.nextSibling(idx, end);
      final value = FjValue(tape, idx);
      result[key] = _valueToDynamic(value);
      idx = tape.nextSibling(idx, end);
    }
    return result;
  }

  List<dynamic> _toJsonArray() {
    final result = <dynamic>[];
    final end = tape.containerEnd(tapeIdx);
    var idx = tape.arrayFirst(tapeIdx);
    while (idx < end) {
      final value = FjValue(tape, idx);
      result.add(_valueToDynamic(value));
      idx = tape.nextSibling(idx, end);
    }
    return result;
  }

  dynamic _valueToDynamic(FjValue v) {
    switch (v.tag) {
      case tagNull:
        return null;
      case tagTrue:
        return true;
      case tagFalse:
        return false;
      case tagI64:
        return v.asInt();
      case tagF64:
        return v.asDouble();
      case tagString:
        return v.asString();
      case tagObject:
        return v._toJsonObject();
      case tagArray:
        return v._toJsonArray();
      default:
        throw FjException(FjError.badHandle);
    }
  }
}

/// FjArray - Iterator-based array access.
///
/// Provides efficient iteration over array elements without indexing overhead.
/// Uses sync* generator for lazy evaluation.
class FjArray extends FjValue {
  const FjArray(super.tape, super.tapeIdx);

  int get firstIdx => tape.arrayFirst(tapeIdx);
  int get endIdx => tape.containerEnd(tapeIdx);

  /// Get element at index (O(n) traversal).
  ///
  /// Note: This is O(n) because the tape is singly-linked.
  /// For sequential access, use the iterable instead.
  FjValue elementAt(int index) {
    var idx = firstIdx;
    for (var i = 0; i < index; i++) {
      idx = tape.nextSibling(idx, endIdx);
    }
    return FjValue(tape, idx);
  }

  /// Number of elements in array.
  @override
  int get length {
    int count = 0;
    var idx = firstIdx;
    while (idx < endIdx) {
      count++;
      idx = tape.nextSibling(idx, endIdx);
    }
    return count;
  }

  /// Index-based access.
  FjValue at(int index) => elementAt(index);

  /// Lazy iterator over array elements.
  ///
  /// Uses sync* for memory-efficient iteration.
  /// Each element is accessed only when iterated.
  ///
  /// Example:
  /// ```dart
  /// for (final item in doc["items"].asArray()) {
  ///   process(item.asString());
  /// }
  /// ```
  Iterable<FjValue> get iterable sync* {
    var idx = firstIdx;
    while (idx < endIdx) {
      yield FjValue(tape, idx);
      idx = tape.nextSibling(idx, endIdx);
    }
  }

  /// Materialize array as List.
  ///
  /// Warning: This materializes all elements.
  List<dynamic> toList() {
    final result = <dynamic>[];
    for (final v in iterable) {
      result.add(v.toJson());
    }
    return result;
  }
}
