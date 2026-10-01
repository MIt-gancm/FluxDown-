import 'dart:io';

/// Resolve the application data directory (mobile only).
///
/// | Platform | Directory                                      |
/// |----------|------------------------------------------------|
/// | Android  | `/data/data/com.fluxdown.app/files/fluxdown`   |
/// | iOS      | `$HOME/Library/Application Support/fluxdown`   |
String resolveDataDir() {
  if (Platform.isAndroid) {
    return '/data/data/com.fluxdown.app/files/fluxdown';
  }
  final home = Platform.environment['HOME'] ?? '';
  return '$home/Library/Application Support/fluxdown';
}
