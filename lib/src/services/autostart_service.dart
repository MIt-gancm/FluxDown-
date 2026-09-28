import 'dart:io';

import 'package:launch_at_startup/launch_at_startup.dart';

import 'log_service.dart';

const _tag = 'Autostart';

/// 开机自启条目名：Windows `Run` 值名 / Linux `FluxDown.desktop` 文件名
/// （沿用 launch_at_startup 的 appName，升级后原条目原地改写）。
const autostartAppName = 'FluxDown';

/// 自启条目携带的参数：原生层据此跳过首帧显示（静默驻留托盘）。
const autostartSilentArg = '--silentStart';

const _windowsRunKey = r'HKCU\Software\Microsoft\Windows\CurrentVersion\Run';

/// 开机自启（Flutter 客户端）。
///
/// 与 GPUI agent（`native/agent/src/platform/autostart.rs`）共享同一套语义：
/// - **已启用** = 本应用的自启条目存在，且没有被系统级开关禁用（Windows
///   `Explorer\StartupApproved\Run` 首字节为奇数；XDG `Hidden=true` /
///   `X-GNOME-Autostart-enabled=false`）。
/// - **用户在应用内开启** = 写入条目并清除系统级禁用标记；**关闭** = 删除条目。
/// - **启动时自动刷新**（旧路径 / 缺 `--silentStart` 的条目迁移）只改写启动目标
///   （Windows `Run` 值 / `.desktop` 的 `Exec` 行），**绝不**改动系统级启用状态：
///   用户在系统设置 / 任务管理器 / 桌面环境里关掉的自启不能被应用重新打开。
///
/// Windows、macOS 的启用 / 禁用 / 检测走 launch_at_startup；Linux 由本类直接维护
/// XDG autostart 文件（插件只看文件是否存在，且每次启用整份重写会抹掉用户的禁用
/// 标记；AppImage 下还会写入临时挂载路径）。
class AutostartService {
  AutostartService._();

  static final AutostartService instance = AutostartService._();

  late final LinuxAutostart _linux = LinuxAutostart(
    configDir: _xdgConfigDir(),
    executable: _linuxExecutable(),
  );

  static bool get _desktop =>
      Platform.isWindows || Platform.isMacOS || Platform.isLinux;

  /// 注册插件的平台实现；必须在任何读写之前调用一次。
  void setup() {
    if (!Platform.isWindows && !Platform.isMacOS) return;
    launchAtStartup.setup(
      appName: autostartAppName,
      // Windows 路径加引号，避免空格截断；插件把 appPath 与 args 以空格拼成 Run 值，
      // 结果与 [windowsRunValue] 逐字一致。
      appPath: Platform.isWindows
          ? '"${Platform.resolvedExecutable}"'
          : Platform.resolvedExecutable,
      args: const [autostartSilentArg],
    );
  }

  Future<bool> isEnabled() async {
    if (Platform.isLinux) return _linux.isEnabled();
    if (!_desktop) return false;
    return launchAtStartup.isEnabled();
  }

  Future<void> enable() async {
    if (Platform.isLinux) return _linux.enable();
    if (!_desktop) return;
    await launchAtStartup.enable();
  }

  Future<void> disable() async {
    if (Platform.isLinux) return _linux.disable();
    if (!_desktop) return;
    await launchAtStartup.disable();
  }

  /// 启动时把已存在的自启条目改写为当前可执行文件 + `--silentStart`，
  /// 不存在则什么都不做；不改变系统级启用状态。
  Future<void> refreshRegistration() async {
    if (Platform.isWindows) {
      await _refreshWindowsRunValue();
    } else if (Platform.isLinux) {
      _linux.refreshRegistration();
    }
    // macOS 走 SMAppService 登录项，没有可改写的启动目标。
  }

  /// 只改写 `Run` 值；`StartupApproved\Run` 保持原样（插件的 `enable()` 会把它
  /// 强制写成「已启用」，因此迁移不能复用 `enable()`）。
  Future<void> _refreshWindowsRunValue() async {
    final query = await Process.run('reg', [
      'query',
      _windowsRunKey,
      '/v',
      autostartAppName,
    ]);
    if (query.exitCode != 0) return;
    final expected = windowsRunValue(Platform.resolvedExecutable);
    final current = parseRegQueryValue(
      query.stdout as String,
      autostartAppName,
    );
    if (current == expected) return;
    final add = await Process.run('reg', [
      'add',
      _windowsRunKey,
      '/v',
      autostartAppName,
      '/t',
      'REG_SZ',
      '/d',
      expected,
      '/f',
    ]);
    if (add.exitCode != 0) {
      logInfo(_tag, 'rewrite Run value failed: ${add.stderr}');
      return;
    }
    logInfo(_tag, 'migrated Run value to --silentStart form');
  }

  static String _xdgConfigDir() {
    final env = Platform.environment;
    final xdg = env['XDG_CONFIG_HOME'];
    if (xdg != null && xdg.startsWith('/')) return xdg;
    return '${env['HOME'] ?? ''}/.config';
  }

  /// AppImage 运行时 `resolvedExecutable` 是每次启动都不同的临时挂载点
  /// （`/tmp/.mount_*`），退出即失效；自启必须指向 `$APPIMAGE` 镜像本身。
  static String _linuxExecutable() {
    final appImage = Platform.environment['APPIMAGE'];
    if (appImage != null && appImage.isNotEmpty) return appImage;
    return Platform.resolvedExecutable;
  }
}

/// Windows `Run` 值：`"<exe>" --silentStart`（与 `installer/windows/setup.iss` 一致）。
String windowsRunValue(String executable) =>
    '"$executable" $autostartSilentArg';

/// 从 `reg query <key> /v <name>` 的输出取出值数据；找不到返回 null。
String? parseRegQueryValue(String stdout, String valueName) {
  final pattern = RegExp(
    '^\\s*${RegExp.escape(valueName)}\\s+REG_\\w+\\s+(.*?)\\s*\$',
    caseSensitive: false,
    multiLine: true,
  );
  return pattern.firstMatch(stdout)?.group(1);
}

/// XDG autostart 条目（`<configDir>/autostart/FluxDown.desktop`）。
class LinuxAutostart {
  LinuxAutostart({required this.configDir, required this.executable});

  final String configDir;
  final String executable;

  File get _file => File('$configDir/autostart/$autostartAppName.desktop');

  bool isEnabled() {
    final content = _read();
    return content != null && xdgAutostartEntryEnabled(content);
  }

  /// 整份重写：用户在应用内明确开启，连同 `Hidden` 等禁用标记一起清掉。
  void enable() {
    final file = _file;
    file.parent.createSync(recursive: true);
    file.writeAsStringSync(xdgAutostartEntry(executable));
  }

  void disable() {
    final file = _file;
    if (file.existsSync()) file.deleteSync();
  }

  /// 只替换 `Exec` 行，其余键（含 `Hidden` / `X-GNOME-Autostart-enabled`）原样保留。
  void refreshRegistration() {
    final content = _read();
    if (content == null) return;
    final updated = xdgRetargetExec(content, xdgExecLine(executable));
    if (updated == null) return;
    _file.writeAsStringSync(updated);
    logInfo(_tag, 'migrated autostart Exec to $executable');
  }

  String? _read() {
    try {
      return _file.readAsStringSync();
    } on FileSystemException {
      return null;
    }
  }
}

/// 按 Desktop Entry 规范为 `Exec` 引用并转义参数（与 agent `exec_quote` 同规则）。
String _execQuote(String value) {
  final out = StringBuffer('"');
  for (final rune in value.runes) {
    final ch = String.fromCharCode(rune);
    if (ch == '"' || ch == '`' || ch == r'$' || ch == r'\') out.write(r'\');
    out.write(ch);
  }
  out.write('"');
  return out.toString();
}

String xdgExecLine(String executable) =>
    'Exec=${_execQuote(executable)} $autostartSilentArg';

String xdgAutostartEntry(String executable) =>
    '[Desktop Entry]\n'
    'Type=Application\n'
    'Name=$autostartAppName\n'
    'Comment=Free IDM-alternative download manager\n'
    '${xdgExecLine(executable)}\n'
    'StartupNotify=false\n'
    'Terminal=false\n';

const _desktopEntryGroup = '[Desktop Entry]';

/// 逐行解析 `[Desktop Entry]` 组内的 `key=value`；`=` 两侧空白忽略。
({String key, String value})? _entryKeyValue(String line) {
  final index = line.indexOf('=');
  if (index <= 0) return null;
  return (
    key: line.substring(0, index).trim(),
    value: line.substring(index + 1).trim(),
  );
}

/// `[Desktop Entry]` 组内没有 `Hidden=true` 且没有 `X-GNOME-Autostart-enabled=false`。
bool xdgAutostartEntryEnabled(String content) {
  var inEntry = false;
  for (final raw in content.split('\n')) {
    final line = raw.trim();
    if (line.startsWith('[')) {
      inEntry = line == _desktopEntryGroup;
      continue;
    }
    if (!inEntry) continue;
    final kv = _entryKeyValue(line);
    if (kv == null) continue;
    if (kv.key == 'Hidden' && kv.value == 'true') return false;
    if (kv.key == 'X-GNOME-Autostart-enabled' && kv.value == 'false') {
      return false;
    }
  }
  return true;
}

/// 把 `[Desktop Entry]` 组的 `Exec` 行换成 [execLine]（缺失则插在组头之后），
/// 其余行逐字保留；内容无变化或没有 `[Desktop Entry]` 组时返回 null。
String? xdgRetargetExec(String content, String execLine) {
  final lines = content.split('\n');
  var inEntry = false;
  var header = -1;
  for (var i = 0; i < lines.length; i++) {
    final line = lines[i].trim();
    if (line.startsWith('[')) {
      inEntry = line == _desktopEntryGroup;
      if (inEntry && header < 0) header = i;
      continue;
    }
    if (!inEntry || _entryKeyValue(line)?.key != 'Exec') continue;
    if (lines[i] == execLine) return null;
    lines[i] = execLine;
    return lines.join('\n');
  }
  if (header < 0) return null;
  lines.insert(header + 1, execLine);
  return lines.join('\n');
}
