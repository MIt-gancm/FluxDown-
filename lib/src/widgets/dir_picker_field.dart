import 'package:flutter/widgets.dart';
import 'package:shadcn_ui/shadcn_ui.dart';

import '../i18n/locale_provider.dart';
import '../theme/app_colors.dart';

/// 目录选择器一体化控件。
///
/// 外观是一个输入框，路径文本在左，浏览按钮嵌入右侧，
/// 中间用竖分隔线分开，视觉上是一个整体。
class DirPickerField extends StatelessWidget {
  final String path;
  final String? placeholder;
  final bool enabled;
  final VoidCallback? onTap;

  const DirPickerField({
    super.key,
    required this.path,
    this.placeholder,
    this.enabled = true,
    this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    final s = LocaleScope.of(context);
    final hasPath = path.isNotEmpty;
    final displayText = hasPath ? path : (placeholder ?? s.selectSaveDir);

    return Container(
      height: 36,
      decoration: BoxDecoration(
        color: c.surface1,
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: c.border, width: 1),
      ),
      child: Row(
        children: [
          // 路径文本
          Expanded(
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12),
              child: Text(
                displayText,
                style: TextStyle(
                  fontSize: 13,
                  color: hasPath ? c.textPrimary : c.textMuted,
                ),
                overflow: TextOverflow.ellipsis,
                maxLines: 1,
              ),
            ),
          ),
          // 竖分隔线
          Container(width: 1, height: 20, color: c.border),
          // 浏览按钮区域
          GestureDetector(
            onTap: enabled ? onTap : null,
            child: MouseRegion(
              cursor: enabled
                  ? SystemMouseCursors.click
                  : SystemMouseCursors.basic,
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 10),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(
                      LucideIcons.folderOpen,
                      size: 14,
                      color: enabled
                          ? c.textSecondary
                          : c.textMuted.withValues(alpha: 0.5),
                    ),
                    const SizedBox(width: 5),
                    Text(
                      s.browse,
                      style: TextStyle(
                        fontSize: 12.5,
                        color: enabled
                            ? c.textSecondary
                            : c.textMuted.withValues(alpha: 0.5),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
