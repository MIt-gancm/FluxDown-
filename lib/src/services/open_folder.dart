import 'package:url_launcher/url_launcher.dart';

import '../bindings/bindings.dart';

/// 在文件管理器中打开文件所在目录（尽可能选中文件）或目录本身。
///
/// 实际实现完全在 Rust 端：见 native/hub/src/reveal_file.rs。
/// Rust 端会按以下顺序决定：
///   1. 用户在设置中配置了自定义命令模板（reveal_file_cmd / open_dir_cmd）
///      → 走模板（cmd /c 或 sh -c），支持任意第三方文件管理器
///   2. 否则走平台默认：
///      Windows: explorer /select 或 cmd /c start
///      macOS:   open -R 或 open
///      Linux:   D-Bus FileManager1.ShowItems 或 xdg-open
///
/// [filePath] 可以是文件路径或目录路径——Rust 端会用 fs::metadata 自动判定。
Future<void> openFolder(String filePath) async {
  RevealFile(path: filePath).sendSignalToRust();
}

/// 用系统默认程序打开文件。
/// Windows 上 launchUrl(file://) 走 ShellExecuteW("open", ...)，
/// 通过完整的注册表查找链解析关联应用，比 cmd /c start 更可靠。
Future<void> openFile(String filePath) async {
  await launchUrl(Uri.file(filePath));
}
