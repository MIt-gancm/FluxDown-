import 'package:flutter/widgets.dart';

import '../bindings/bindings.dart';
import '../mobile/sheets/mobile_bt_file_sheet.dart';

void showBtFileSelectionDialog(
  BuildContext context, {
  required String taskId,
  required int totalBytes,
  required List<BtFileEntry> files,
  VoidCallback? onClosed,
}) {
  showMobileBtFileSheet(
    context,
    taskId: taskId,
    files: files,
    onClosed: onClosed,
  );
}
