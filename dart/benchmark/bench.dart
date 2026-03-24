import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

import '../fast_json.dart';

void main(List<String> args) {
  registerDefaultDecoders();
  final dumpFiles = args.contains('--dump');

  benchmark1MB(dumpFiles: dumpFiles);
  benchmark10MB(dumpFiles: dumpFiles);
}

void benchmark1MB({bool dumpFiles = false}) {
  final payload =
      _generatePayload(1024 * 1024, '1mb_benchmark.json', dumpFiles);
  print('=== 1 MB Payload Benchmark ===');
  _runBenchmark('FastJson', payload);
  _runBenchmark('dart:convert', payload);
  print('');
}

void benchmark10MB({bool dumpFiles = false}) {
  final payload =
      _generatePayload(10 * 1024 * 1024, '10mb_benchmark.json', dumpFiles);
  print('=== 10 MB Payload Benchmark ===');
  _runBenchmark('FastJson', payload);
  _runBenchmark('dart:convert', payload);
  print('');
}

void benchmark50MB({bool dumpFiles = false}) {
  final payload = _generatePayload(50 * 1024 * 1024, '50mb_benchmark.json', false);
  print('=== 50 MB Payload Benchmark ===');
  _runBenchmark('FastJson', payload);
  _runBenchmark('dart:convert', payload);
}

void _runBenchmark(String name, Uint8List payload) {
  final iterations = 10;
  final sw = Stopwatch()..start();
  for (var i = 0; i < iterations; i++) {
    if (name == 'FastJson') {
      final doc = FastJson.parse(payload);
      doc?.dispose();
    } else {
      jsonDecode(utf8.decode(payload));
    }
  }
  sw.stop();

  final elapsed = sw.elapsedMilliseconds;
  final mbPerSec =
      (payload.length * iterations) / (1024 * 1024) / (elapsed / 1000);
  print('$name: ${elapsed}ms total, ${mbPerSec.toStringAsFixed(2)} MB/s');
}

Uint8List _generatePayload(int targetSize, String filename, bool dumpFile) {
  final random = Random(42);
  final buffer = StringBuffer();
  buffer.write('{"data":[');

  final itemSize = 50;
  final itemCount = max(1, targetSize ~/ itemSize);

  for (var i = 0; i < itemCount; i++) {
    if (i > 0) buffer.write(',');
    buffer.write('{');
    buffer.write('"id":$i,');
    buffer.write('"name":"item_$i",');
    buffer.write('"value":${random.nextDouble() * 1000},');
    buffer.write('"active":${random.nextBool()},');
    buffer.write('"tags":["tag1","tag2","tag3"]}');
  }

  buffer.write(
    '],"meta":{"version":"1.0","generated":${DateTime.now().millisecondsSinceEpoch}}}',
  );

  final jsonStr = buffer.toString();
  final result = Uint8List.fromList(jsonStr.codeUnits);

  if (result.length < targetSize) {
    final padding = List.generate(
      targetSize - result.length,
      (_) => ' '.codeUnitAt(0),
    );
    final padded = Uint8List(targetSize);
    padded.setRange(0, result.length, result);
    padded.setRange(result.length, targetSize, padding);

    if (dumpFile) {
      File(filename).writeAsStringSync(utf8.decode(padded));
      print('Dumped JSON to $filename (${padded.length} bytes)');
    }

    return padded;
  }

  if (dumpFile) {
    File(filename).writeAsStringSync(jsonStr);
    print('Dumped JSON to $filename (${result.length} bytes)');
  }

  return result;
}
