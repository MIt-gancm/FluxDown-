import 'package:flutter/widgets.dart';

import '../bindings/bindings.dart';
import '../mobile/sheets/mobile_hls_quality_sheet.dart';

void showHlsQualityDialog(
  BuildContext context, {
  required String taskId,
  required List<HlsQualityOption> options,
}) {
  showMobileHlsQualitySheet(context, taskId: taskId, options: options);
}
