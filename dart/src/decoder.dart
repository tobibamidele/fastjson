/// Decoder Layer - Phase 1/4: Typed Decoding Infrastructure
///
/// This layer provides infrastructure for typed deserialization.
/// Phase 4 will add codegen support to generate type adapters automatically.
///
/// Architecture Compliance:
/// - Dart API Layer (Layer 3 of 3)
/// - Reflection-free by default
/// - Codegen hooks for Phase 4
///
/// Design Philosophy:
/// - No reflection (dart:mirrors) for performance
/// - Type adapters registered at startup
/// - Codegen will generate adapters in Phase 4
///
/// Memory Safety:
/// - Decoder functions receive FjValue views (no ownership transfer)
/// - Native memory is released via FjDocument.dispose()
///
/// Phase 4 Additions (planned):
/// - @JsonSerializable annotation
/// - build_runner integration
/// - Generated .g.dart adapter files

import 'ffi.dart';
import 'models.dart';
import 'api.dart';

/// Options for deserialization behavior.
class FjDeserializeOptions {
  /// Coerce types where sensible (e.g., string "123" to int 123).
  final bool coerceTypes;

  /// Allow null values where type expects non-null.
  final bool allowNull;

  /// Strict mode: error on type coercion.
  final bool strict;

  const FjDeserializeOptions({
    this.coerceTypes = true,
    this.allowNull = true,
    this.strict = false,
  });
}

/// Context passed to type adapters during decoding.
///
/// Provides access to document and options for adapter customization.
class FjDeserializationContext {
  final FjDocument doc;
  final FjDeserializeOptions options;

  FjDeserializationContext(
    this.doc, {
    this.options = const FjDeserializeOptions(),
  });
}

/// Type adapter function signature.
///
/// Adapters convert FjValue to typed objects.
///
/// Example:
/// ```dart
/// FjTypeAdapter<User> adapter = (value, ctx) {
///   return User(
///     name: value.getField('name').asString(),
///     age: value.getField('age').asInt(),
///   );
/// };
/// FjTypeRegistry.register<User>(adapter);
/// ```
typedef FjTypeAdapter<T> = T Function(
    FjValue value, FjDeserializationContext ctx);

/// Registry for type adapters.
///
/// Phase 4: This will be populated by codegen.
class FjTypeRegistry {
  static final _adapters = <Type, FjTypeAdapter>{};

  /// Register a type adapter.
  static void register<T>(FjTypeAdapter<T> adapter) {
    _adapters[T] = adapter;
  }

  /// Get adapter for type T.
  static FjTypeAdapter<T>? get<T>() {
    return _adapters[T] as FjTypeAdapter<T>?;
  }

  /// Try to adapt value to type T.
  /// Returns null if no adapter registered.
  static T? tryAdapt<T>(FjValue value, FjDeserializationContext ctx) {
    final adapter = _adapters[T];
    if (adapter != null) {
      return adapter(value, ctx);
    }
    return null;
  }

  /// Adapt value to type T.
  /// Throws if no adapter registered.
  static T adapt<T>(FjValue value, FjDeserializationContext ctx) {
    final adapter = _adapters[T];
    if (adapter != null) {
      return adapter(value, ctx);
    }
    throw UnsupportedError('No adapter registered for type $T');
  }
}

/// Dynamic adapter for untyped decoding.
///
/// Materializes values recursively without type information.
/// Used as fallback when no specific adapter is registered.
class FjDynamicAdapter {
  /// Decode value to dynamic Dart object.
  ///
  /// Recursively materializes:
  /// - Objects -> Map<String, dynamic>
  /// - Arrays -> List<dynamic>
  /// - Primitives -> int/double/bool/String/null
  static dynamic fromJson(FjValue value) {
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
        return _objectFromJson(value);
      case tagArray:
        return _arrayFromJson(value);
      default:
        throw FjException(FjError.badHandle);
    }
  }

  static Map<String, dynamic> _objectFromJson(FjValue value) {
    final result = <String, dynamic>{};
    final end = value.tape.containerEnd(value.tapeIdx);
    var idx = value.tapeIdx + 1;
    while (idx < end) {
      final key = FjValue(value.tape, idx).asString();
      idx = value.tape.nextSibling(idx, end);
      final fieldValue = FjValue(value.tape, idx);
      result[key] = fromJson(fieldValue);
      idx = value.tape.nextSibling(idx, end);
    }
    return result;
  }

  static List<dynamic> _arrayFromJson(FjValue value) {
    final result = <dynamic>[];
    final end = value.tape.containerEnd(value.tapeIdx);
    var idx = value.tape.arrayFirst(value.tapeIdx);
    while (idx < end) {
      final item = FjValue(value.tape, idx);
      result.add(fromJson(item));
      idx = value.tape.nextSibling(idx, end);
    }
    return result;
  }
}

/// Decode document to type T.
///
/// Uses registered adapter if available, otherwise falls back to dynamic.
T docDecode<T>(FjDocument doc) {
  final adapter = FjTypeRegistry.get<T>();
  if (adapter != null) {
    return adapter(doc, FjDeserializationContext(doc));
  }
  return FjDynamicAdapter.fromJson(doc) as T;
}
