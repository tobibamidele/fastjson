/// API Layer - Phase 1: Public Facade
///
/// This layer provides the public API for the FastJson parser.
/// Clean entry point for users with sensible defaults.
///
/// Architecture Compliance:
/// - Dart API Layer (Layer 3 of 3)
/// - Provides FastJson.parse() as the main entry point
/// - FjDocument owns native memory and provides dispose()
/// - FjDecoder provides typed decoding registry
///
/// Memory Model:
/// - FjDocument owns NativeTape handle
/// - User MUST call dispose() to release native memory
/// - Recommended: use 'using' pattern or try-finally
///
/// Example:
/// ```dart
/// final doc = FastJson.parse(bytes);
/// if (doc != null) {
///   try {
///     final name = doc["user"]["name"].asString();
///     print(name);
///   } finally {
///     doc.dispose();
///   }
/// }
/// ```

import 'dart:typed_data';

import 'ffi.dart';
import 'models.dart';

/// Main entry point for parsing JSON.
///
/// Parses JSON bytes into a lazy document that can be navigated
/// without full materialization. Native memory is owned by the
/// returned FjDocument.
///
/// Usage:
/// ```dart
/// final doc = FastJson.parse(bytes);
/// if (doc != null) {
///   try {
///     final value = doc["key"].asString();
///   } finally {
///     doc.dispose();
///   }
/// }
/// ```
///
/// Returns null if parsing fails.
class FastJson {
  FastJson._();

  static FjDocument? parse(Uint8List bytes, {bool useSimd = false}) {
    final tape = NativeTape.parse(bytes, useSimd: useSimd);
    if (tape == null) return null;
    return FjDocument(tape);
  }

  /// Parse from a String (convenience method).
  ///
  /// Note: For large JSON, prefer parsing from bytes directly
  /// to avoid the Dart String -> UTF-8 conversion.
  static FjDocument? parseString(String json) {
    return parse(Uint8List.fromList(json.codeUnits));
  }
}

/// FjDocument - Root of parsed JSON document.
///
/// Owns the native tape handle and provides access to the root value.
/// The document must be disposed to release native memory.
///
/// Design: Lazy access model
/// - Document is parsed once into tape structure
/// - Values are accessed on-demand via FjValue views
/// - No full DOM materialization unless explicitly requested
///
/// Memory Safety:
/// - MUST call dispose() when done
/// - Consider using 'using' pattern or try-finally
class FjDocument extends FjValue {
  final NativeTape _tape;

  FjDocument(this._tape) : super(_tape, 0);

  @override
  int get tapeIdx => 0;

  /// Release native memory.
  ///
  /// IMPORTANT: Must be called to prevent memory leaks.
  /// Call this when you're done with the document.
  ///
  /// Example:
  /// ```dart
  /// final doc = FastJson.parse(bytes);
  /// try {
  ///   // use doc
  /// } finally {
  ///   doc.dispose();
  /// }
  /// ```
  void dispose() {
    _tape.free();
  }
}

/// Type alias for decoder functions.
typedef Decodable<T> = T Function(FjValue value);

/// FjDecoder - Registry for typed decoding.
///
/// Provides a way to register custom decoders for specific types.
/// Useful for domain-specific deserialization.
///
/// Phase 4 will add codegen support to generate these automatically.
///
/// Example:
/// ```dart
/// FjDecoder.register<User>((v) => User(
///   name: v.getField('name').asString(),
///   age: v.getField('age').asInt(),
/// ));
/// final user = FjDecoder.decode<User>(doc["user"]);
/// ```
class FjDecoder {
  static final _registry = <Type, Decodable>{};

  /// Register a decoder for a type.
  static void register<T>(Decodable<T> decoder) {
    _registry[T] = decoder;
  }

  /// Decode a value to type T using registered decoder.
  ///
  /// Throws UnsupportedError if no decoder is registered for T.
  static T decode<T>(FjValue value) {
    final decoder = _registry[T];
    if (decoder != null) {
      return decoder(value);
    }
    throw UnsupportedError('No decoder registered for type $T');
  }

  /// Try to decode, returning null if no decoder registered.
  static T? tryDecode<T>(FjValue value) {
    final decoder = _registry[T];
    if (decoder != null) {
      return decoder(value);
    }
    return null;
  }

  /// Decode any value to dynamic (materializes full subtree).
  ///
  /// Warning: This recursively materializes the value tree.
  /// For lazy access, use FjValue methods directly.
  static dynamic decodeDynamic(FjValue value) {
    switch (value.tag) {
      case tagNull:
        return null;
      case tagTrue:
        return true;
      case tagFalse:
        return false;
      case tagI64:
        return value.asInt();
      case tagF64:
        return value.asDouble();
      case tagString:
        return value.asString();
      case tagObject:
        return value.toJson();
      case tagArray:
        return value.toJson();
      default:
        throw FjException(FjError.badHandle);
    }
  }
}

/// Register default decoders for common types.
///
/// Call this once at application startup to enable
/// automatic decoding of basic types.
void registerDefaultDecoders() {
  FjDecoder.register<String>((v) => v.asString());
  FjDecoder.register<int>((v) => v.asInt());
  FjDecoder.register<double>((v) => v.asDouble());
  FjDecoder.register<bool>((v) => v.asBool());
  FjDecoder.register<List<dynamic>>((v) => v.asArray().toList());
  FjDecoder.register<Map<String, dynamic>>(
    (v) => v.toJson() as Map<String, dynamic>,
  );
}
