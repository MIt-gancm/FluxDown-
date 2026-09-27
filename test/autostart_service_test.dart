import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:flux_down/src/services/autostart_service.dart';

void main() {
  group('parseRegQueryValue', () {
    test('extracts quoted command line from reg query output', () {
      const stdout =
          '\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n'
          '    FluxDown    REG_SZ    "C:\\Program Files\\FluxDown\\FluxDown.exe" --silentStart\r\n'
          '\r\n';
      expect(
        parseRegQueryValue(stdout, 'FluxDown'),
        windowsRunValue(r'C:\Program Files\FluxDown\FluxDown.exe'),
      );
    });

    test('returns null when value name is absent', () {
      const stdout = '    Other    REG_SZ    "C:\\x.exe"\r\n';
      expect(parseRegQueryValue(stdout, 'FluxDown'), isNull);
    });
  });

  group('xdgAutostartEntryEnabled', () {
    test('fresh entry is enabled', () {
      expect(xdgAutostartEntryEnabled(xdgAutostartEntry('/opt/a')), isTrue);
    });

    test('Hidden=true disables (XDG spec, XFCE/LXQt)', () {
      expect(
        xdgAutostartEntryEnabled('[Desktop Entry]\nExec=a\nHidden = true\n'),
        isFalse,
      );
    });

    test('X-GNOME-Autostart-enabled=false disables', () {
      expect(
        xdgAutostartEntryEnabled(
          '[Desktop Entry]\nExec=a\nX-GNOME-Autostart-enabled=false\n',
        ),
        isFalse,
      );
    });

    test('keys outside [Desktop Entry] are ignored', () {
      expect(
        xdgAutostartEntryEnabled(
          '[Desktop Entry]\nExec=a\n[Desktop Action x]\nHidden=true\n',
        ),
        isTrue,
      );
    });
  });

  group('xdgRetargetExec', () {
    test('replaces only Exec and keeps disable markers', () {
      const content =
          '[Desktop Entry]\nType=Application\nExec=/tmp/.mount_x/flux_down\n'
          'Hidden=true\n';
      final updated = xdgRetargetExec(content, xdgExecLine('/opt/a b'));
      expect(
        updated,
        '[Desktop Entry]\nType=Application\n'
        'Exec="/opt/a b" --silentStart\nHidden=true\n',
      );
    });

    test('returns null when Exec already matches', () {
      final content = xdgAutostartEntry('/opt/a');
      expect(xdgRetargetExec(content, xdgExecLine('/opt/a')), isNull);
    });

    test('inserts Exec after group header when missing', () {
      expect(
        xdgRetargetExec('[Desktop Entry]\nName=x\n', 'Exec=y'),
        '[Desktop Entry]\nExec=y\nName=x\n',
      );
    });
  });

  group('LinuxAutostart', () {
    late Directory dir;
    late File file;

    setUp(() {
      dir = Directory.systemTemp.createTempSync('fluxdown-autostart-');
      file = File('${dir.path}/autostart/FluxDown.desktop');
    });

    tearDown(() => dir.deleteSync(recursive: true));

    LinuxAutostart at(String exe) =>
        LinuxAutostart(configDir: dir.path, executable: exe);

    test('startup refresh never re-enables an entry the user disabled', () {
      final old = at('/tmp/.mount_old/flux_down')..enable();
      file.writeAsStringSync('${file.readAsStringSync()}Hidden=true\n');
      expect(old.isEnabled(), isFalse);

      final current = at('/home/u/FluxDown.AppImage')..refreshRegistration();

      expect(current.isEnabled(), isFalse);
      final content = file.readAsStringSync();
      expect(content, contains(xdgExecLine('/home/u/FluxDown.AppImage')));
      expect(content, contains('Hidden=true'));
    });

    test('refresh does not create a missing entry', () {
      at('/opt/fluxdown/flux_down').refreshRegistration();
      expect(file.existsSync(), isFalse);
    });

    test('explicit enable clears disable markers; disable removes entry', () {
      final autostart = at('/opt/fluxdown/flux_down');
      file.parent.createSync(recursive: true);
      file.writeAsStringSync('[Desktop Entry]\nExec=x\nHidden=true\n');

      autostart.enable();
      expect(autostart.isEnabled(), isTrue);

      autostart.disable();
      expect(file.existsSync(), isFalse);
      expect(autostart.isEnabled(), isFalse);
    });
  });
}
