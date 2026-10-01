// Tests for KvStore (lib/src/services/kv_store.dart) — the synchronous
// facade over SharedPreferences. SharedPreferences is driven through
// `setMockInitialValues`; `debugReset` isolates each test.

import 'package:flutter_test/flutter_test.dart';
import 'package:flux_down/src/services/kv_store.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  Future<void> initWith(Map<String, Object> values) async {
    SharedPreferences.setMockInitialValues(values);
    KvStore.instance.debugReset();
    await KvStore.instance.init();
  }

  setUp(() => initWith({}));
  tearDown(() => KvStore.instance.debugReset());

  test('write then read back synchronously', () async {
    await KvStore.instance.setString('name', 'flux');
    await KvStore.instance.setBool('enabled', true);
    await KvStore.instance.setDouble('ratio', 3.5);

    expect(KvStore.instance.getString('name'), 'flux');
    expect(KvStore.instance.getBool('enabled'), true);
    expect(KvStore.instance.getDouble('ratio'), 3.5);
  });

  test('written values survive a restart (reset + re-init)', () async {
    await KvStore.instance.setString('name', 'flux');
    await KvStore.instance.setDouble('width', 100.0);

    KvStore.instance.debugReset();
    await KvStore.instance.init();

    expect(KvStore.instance.getString('name'), 'flux');
    expect(KvStore.instance.getDouble('width'), isA<double>());
    expect(KvStore.instance.getDouble('width'), 100.0);
  });

  test('init loads pre-existing preferences into the cache', () async {
    await initWith({'k': 'v', 'b': true, 'd': 1.5});

    expect(KvStore.instance.getString('k'), 'v');
    expect(KvStore.instance.getBool('b'), true);
    expect(KvStore.instance.getDouble('d'), 1.5);
  });

  test(
    'remove clears the key and the removal persists across restart',
    () async {
      await KvStore.instance.setString('token', 'secret');
      await KvStore.instance.remove('token');
      expect(KvStore.instance.getString('token'), isNull);

      KvStore.instance.debugReset();
      await KvStore.instance.init();
      expect(KvStore.instance.getString('token'), isNull);
    },
  );

  test('values are type-isolated across getters', () async {
    await KvStore.instance.setString('s', 'value');
    await KvStore.instance.setDouble('d', 1.25);

    expect(KvStore.instance.getBool('s'), isNull);
    expect(KvStore.instance.getDouble('s'), isNull);
    expect(KvStore.instance.getString('d'), isNull);
    expect(KvStore.instance.getBool('d'), isNull);
    expect(KvStore.instance.getString('missing'), isNull);
  });
}
