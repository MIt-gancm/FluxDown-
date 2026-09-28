// 插件设置分类 body：已安装插件管理（启用/设置/卸载） + 安装区
// （zip 上传 / 目录 + 开发模式）+ 插件市场浏览/安装。

import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart' show SelectableText;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:shadcn_ui/shadcn_ui.dart';
import 'package:qr_flutter/qr_flutter.dart';
import 'flux_sonner.dart';
import 'package:url_launcher/url_launcher.dart';

import '../bindings/bindings.dart';
import '../i18n/locale_provider.dart';
import '../models/plugin_provider.dart';
import '../services/file_picker_service.dart';
import '../theme/app_colors.dart';
import '../theme/app_metrics.dart';
import 'dir_picker_field.dart';
import 'plugin_detail_dialog.dart';
import 'plugin_setting_form.dart';

class PluginListView extends StatefulWidget {
  final PluginProvider provider;

  /// 安装的插件缺基础组件（ffmpeg/yt-dlp）时「前往组件设置」的跳转回调
  /// （由设置页注入，切到「扩展 → 组件」Tab）。
  final VoidCallback? onNavigateToComponents;

  const PluginListView({
    super.key,
    required this.provider,
    this.onNavigateToComponents,
  });

  @override
  State<PluginListView> createState() => _PluginListViewState();
}

class _PluginListViewState extends State<PluginListView> {
  int _lastOpSeq = -1;
  bool _installingZip = false;
  bool _installingDir = false;

  /// 开发模式开关跨页面导航保持（切走设置分类会销毁 State，
  /// 用 static 记住本次会话的选择，避免每次回来都重置为默认开）。
  static bool _devModeSticky = true;
  bool get _devMode => _devModeSticky;
  String _devDirPath = '';
  String _marketQuery = '';
  int _marketLimit = _marketPageSize;

  static const int _marketPageSize = 50;

  @override
  void initState() {
    super.initState();
    _lastOpSeq = widget.provider.opResultSeq;
    widget.provider.addListener(_onProviderChanged);
    widget.provider.requestMarket();
  }

  @override
  void dispose() {
    widget.provider.removeListener(_onProviderChanged);
    super.dispose();
  }

  /// 插件写操作结果的全局提示（save_settings 由对话框自身展示，此处跳过避免重复弹）。
  void _onProviderChanged() {
    if (!mounted) return;
    final seq = widget.provider.opResultSeq;
    if (seq != _lastOpSeq) {
      _lastOpSeq = seq;
      final result = widget.provider.lastOpResult;
      if (result != null && result.op != 'save_settings') {
        _showOpResultToast(result);
      }
    }
    setState(() {});
  }

  void _showOpResultToast(PluginOpResult result) {
    final s = currentS;
    if (result.ok) {
      final message = switch (result.op) {
        'install' || 'market_install' => s.pluginOpInstallSuccess,
        'uninstall' => s.pluginOpUninstallSuccess,
        _ => null,
      };
      if (message == null) return;
      FluxSonner.of(context).show(
        ShadToast(title: Text(message), duration: const Duration(seconds: 2)),
      );
      // 安装成功但声明权限所需的基础组件缺失 → 弹依赖提醒（提醒式非阻断，
      // 组件缺失时对应 flux.* 能力面 available() 为 false，插件本身可运行）。
      if ((result.op == 'install' || result.op == 'market_install') &&
          result.missingComponents.isNotEmpty) {
        _showDepsReminder(result.missingComponents);
      }
      return;
    }
    final message = switch (result.op) {
      'install' || 'market_install' => s.pluginOpInstallFailed(result.message),
      'uninstall' => s.pluginOpUninstallFailed(result.message),
      'set_enabled' => s.pluginOpEnabledFailed(result.message),
      _ => s.pluginOpGenericFailed(result.message),
    };
    FluxSonner.of(context).show(ShadToast.destructive(title: Text(message)));
  }

  Future<void> _pickZip() async {
    if (_installingZip) return;
    setState(() => _installingZip = true);
    try {
      final files = await FilePickerService.pickFiles(
        dialogTitle: currentS.pluginInstallZipButton,
        allowedExtensions: const ['fxplug', 'zip'],
      );
      final file = files == null || files.isEmpty ? null : files.first;
      if (file != null) {
        final bytes = await file.readAsBytes();
        widget.provider.install(zipBytes: Uint8List.fromList(bytes));
      }
    } on FilePickerException catch (e) {
      if (mounted) {
        FluxSonner.of(context).show(
          ShadToast.destructive(
            title: Text(currentS.pluginInstallZipFailed(e.toString())),
          ),
        );
      }
    } finally {
      if (mounted) setState(() => _installingZip = false);
    }
  }

  Future<void> _pickDevDir() async {
    if (_installingDir) return;
    setState(() => _installingDir = true);
    try {
      final result = await FilePickerService.pickDirectory(
        dialogTitle: currentS.pluginInstallDirPlaceholder,
        initialDirectory: _devDirPath.isNotEmpty ? _devDirPath : null,
      );
      if (result != null && mounted) {
        setState(() => _devDirPath = result);
      }
    } on FilePickerException catch (_) {
      // 用户取消或选择器错误：静默忽略，与 _SaveDirPickerState 行为一致
    } finally {
      if (mounted) setState(() => _installingDir = false);
    }
  }

  void _installDevDir() {
    if (_devDirPath.isEmpty) return;
    widget.provider.install(dirPath: _devDirPath, devMode: _devMode);
    setState(() => _devDirPath = '');
  }

  void _confirmUninstall(PluginInfoSignal plugin) {
    final s = currentS;
    final c = AppColors.of(context);
    showShadDialog(
      context: context,
      barrierColor: c.dialogBarrier,
      animateIn: const [],
      animateOut: const [],
      builder: (ctx) => ShadDialog(
        title: Text(s.pluginUninstallTitle),
        description: Text(s.pluginUninstallMsg(plugin.name)),
        actions: [
          ShadButton.outline(
            onPressed: () => Navigator.of(ctx).pop(),
            child: Text(s.cancel),
          ),
          ShadButton.destructive(
            onPressed: () {
              Navigator.of(ctx).pop();
              widget.provider.uninstall(plugin.identity);
            },
            child: Text(s.pluginUninstallTooltip),
          ),
        ],
      ),
    );
  }

  void _showPluginAuth(PluginInfoSignal plugin) {
    final c = AppColors.of(context);
    showShadDialog(
      context: context,
      barrierColor: c.dialogBarrier,
      animateIn: const [],
      animateOut: const [],
      builder: (_) =>
          _PluginAuthDialog(plugin: plugin, provider: widget.provider),
    );
  }

  /// 组件名 → 设置页展示名（与「组件」分类标题一致）。
  String _componentDisplayName(String component) {
    final s = currentS;
    return switch (component) {
      'ffmpeg' => s.componentsFfmpegTitle,
      'ytdlp' => s.componentsYtdlpTitle,
      _ => component,
    };
  }

  /// 依赖组件缺失提醒：列出缺失组件，可一键跳转组件设置分类。
  void _showDepsReminder(List<String> missing) {
    final s = currentS;
    final c = AppColors.of(context);
    final names = missing.map(_componentDisplayName).join(', ');
    showShadDialog(
      context: context,
      barrierColor: c.dialogBarrier,
      animateIn: const [],
      animateOut: const [],
      builder: (ctx) => ShadDialog(
        title: Text(s.pluginDepsMissingTitle),
        description: Text(s.pluginDepsMissingBody(names)),
        actions: [
          ShadButton.outline(
            onPressed: () => Navigator.of(ctx).pop(),
            child: Text(s.pluginDepsLater),
          ),
          ShadButton(
            onPressed: () {
              Navigator.of(ctx).pop();
              widget.onNavigateToComponents?.call();
            },
            child: Text(s.pluginDepsGoToComponents),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final s = currentS;
    final c = AppColors.of(context);
    final provider = widget.provider;
    final installedIds = provider.plugins.map((p) => p.identity).toSet();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                s.pluginsSectionTitle,
                style: TextStyle(
                  fontSize: 14,
                  fontWeight: FontWeight.w600,
                  color: c.textPrimary,
                ),
              ),
            ),
            Text(
              s.pluginDevModeSwitch,
              style: TextStyle(fontSize: 12, color: c.textSecondary),
            ),
            const SizedBox(width: 6),
            ShadSwitch(
              value: _devMode,
              onChanged: (v) => setState(() => _devModeSticky = v),
            ),
          ],
        ),
        const SizedBox(height: 10),
        _buildInstallArea(context),
        const SizedBox(height: 14),
        if (provider.plugins.isEmpty)
          Text(
            s.pluginsEmpty,
            style: TextStyle(fontSize: 12.5, color: c.textMuted),
          )
        else
          for (final p in provider.plugins)
            Padding(
              padding: const EdgeInsets.only(bottom: 6),
              child: _PluginCard(
                plugin: p,
                provider: provider,
                onUninstall: () => _confirmUninstall(p),
                onAuth: () => _showPluginAuth(p),
              ),
            ),
        const SizedBox(height: 26),
        Text(
          s.marketSectionTitle,
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w600,
            color: c.textPrimary,
          ),
        ),
        const SizedBox(height: 2),
        Text(
          s.marketSectionDesc,
          style: TextStyle(fontSize: 11.5, color: c.textMuted),
        ),
        const SizedBox(height: 10),
        _buildMarketArea(context, installedIds),
      ],
    );
  }

  Widget _buildInstallArea(BuildContext context) {
    final s = currentS;
    final c = AppColors.of(context);
    final m = AppMetrics.of(context);
    return Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: c.surface1,
        borderRadius: m.brDialog,
        border: Border.all(color: c.border, width: 1),
      ),
      child: Row(
        children: [
          ShadButton.outline(
            onPressed: _installingZip ? null : _pickZip,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(LucideIcons.upload, size: 14, color: c.textPrimary),
                const SizedBox(width: 6),
                Text(s.pluginInstallZipButton),
              ],
            ),
          ),
          if (_devMode) ...[
            const SizedBox(width: 10),
            Expanded(
              child: ShadTooltip(
                waitDuration: const Duration(milliseconds: 300),
                builder: (_) => Text(s.pluginInstallDirLabel),
                child: DirPickerField(
                  path: _devDirPath,
                  placeholder: s.pluginInstallDirPlaceholder,
                  enabled: !_installingDir,
                  onTap: _pickDevDir,
                ),
              ),
            ),
            const SizedBox(width: 10),
            ShadButton(
              onPressed: _devDirPath.isEmpty ? null : _installDevDir,
              child: Text(s.pluginInstallDirButton),
            ),
          ] else
            const Spacer(),
        ],
      ),
    );
  }

  Widget _buildMarketArea(BuildContext context, Set<String> installedIds) {
    final s = currentS;
    final c = AppColors.of(context);
    final provider = widget.provider;
    if (provider.marketLoading) {
      return Text(
        s.pluginCommonLoading,
        style: TextStyle(fontSize: 12.5, color: c.textMuted),
      );
    }
    if (provider.marketError.isNotEmpty) {
      return Text(
        s.marketLoadFailed(provider.marketError),
        style: TextStyle(fontSize: 12.5, color: c.statusError),
      );
    }
    final query = _marketQuery.trim().toLowerCase();
    final filtered = query.isEmpty
        ? provider.marketEntries
        : provider.marketEntries.where((e) {
            bool hit(String v) => v.toLowerCase().contains(query);
            return hit(e.name) ||
                hit(e.pluginId) ||
                hit(e.description) ||
                hit(e.author) ||
                e.tags.any(hit);
          }).toList();
    final visible = filtered.length > _marketLimit
        ? filtered.sublist(0, _marketLimit)
        : filtered;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (provider.marketEntries.length > 8 || query.isNotEmpty) ...[
          ShadInput(
            placeholder: Text(s.marketSearchPlaceholder),
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
            onChanged: (v) => setState(() {
              _marketQuery = v;
              _marketLimit = _marketPageSize;
            }),
          ),
          const SizedBox(height: 8),
        ],
        if (provider.marketEntries.isEmpty)
          Text(
            s.marketEmpty,
            style: TextStyle(fontSize: 12.5, color: c.textMuted),
          )
        else if (filtered.isEmpty)
          Text(
            s.marketSearchNoResult,
            style: TextStyle(fontSize: 12.5, color: c.textMuted),
          )
        else ...[
          for (final entry in visible)
            Padding(
              padding: const EdgeInsets.only(bottom: 6),
              child: _MarketCard(
                entry: entry,
                installed: installedIds.contains(entry.pluginId),
                provider: provider,
              ),
            ),
          if (filtered.length > _marketLimit)
            ShadButton.ghost(
              onPressed: () => setState(() => _marketLimit += _marketPageSize),
              child: Text(
                s.marketShowMore(filtered.length - _marketLimit),
                style: TextStyle(fontSize: 12, color: c.accent),
              ),
            ),
        ],
      ],
    );
  }
}

// =============================================================================
// 已安装插件卡片
// =============================================================================

class _PluginCard extends StatelessWidget {
  final PluginInfoSignal plugin;
  final PluginProvider provider;
  final VoidCallback onUninstall;
  final VoidCallback onAuth;

  const _PluginCard({
    required this.plugin,
    required this.provider,
    required this.onUninstall,
    required this.onAuth,
  });

  void _showLoadError(BuildContext context) {
    final s = currentS;
    final c = AppColors.of(context);
    final m = AppMetrics.of(context);
    showShadDialog(
      context: context,
      barrierColor: c.dialogBarrier,
      animateIn: const [],
      animateOut: const [],
      builder: (ctx) => ShadDialog(
        title: Text(s.pluginLoadErrorTitle),
        description: Text(s.pluginLoadErrorBody),
        actions: [
          ShadButton.outline(
            onPressed: () => Navigator.of(ctx).pop(),
            child: Text(s.close),
          ),
          ShadButton(
            onPressed: () {
              Clipboard.setData(ClipboardData(text: plugin.loadError));
              Navigator.of(ctx).pop();
              FluxSonner.of(
                context,
              ).show(ShadToast(title: Text(s.pluginLoadErrorCopied)));
            },
            child: Text(s.pluginLoadErrorCopy),
          ),
        ],
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxHeight: 240),
          child: SingleChildScrollView(
            child: DefaultSelectionStyle(
              selectionColor: m.soft(c.accent),
              cursorColor: c.accent,
              child: SelectableText(
                plugin.loadError,
                style: TextStyle(fontSize: 12, color: c.textSecondary),
              ),
            ),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final s = currentS;
    final c = AppColors.of(context);
    final m = AppMetrics.of(context);
    final loadFailed = plugin.loadStatus == 'Failed';

    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: () => showPluginDetailDialog(
        context,
        name: plugin.name,
        version: plugin.version,
        identity: plugin.identity,
        description: plugin.description,
        homepage: plugin.homepage,
        settingsCount: plugin.settings.length,
        permissions: plugin.permissions,
      ),
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          decoration: BoxDecoration(
            color: c.surface1,
            borderRadius: m.brDialog,
            border: Border.all(color: c.border, width: 1),
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Wrap(
                      crossAxisAlignment: WrapCrossAlignment.center,
                      spacing: 8,
                      runSpacing: 4,
                      children: [
                        Text(
                          plugin.name,
                          style: TextStyle(
                            fontSize: 13.5,
                            fontWeight: FontWeight.w600,
                            color: c.textPrimary,
                          ),
                        ),
                        Text(
                          'v${plugin.version}',
                          style: TextStyle(fontSize: 11, color: c.textMuted),
                        ),
                        if (plugin.homepage.isNotEmpty)
                          GestureDetector(
                            onTap: () => launchUrl(Uri.parse(plugin.homepage)),
                            child: Text(
                              plugin.homepage,
                              style: TextStyle(fontSize: 11, color: c.accent),
                            ),
                          ),
                        if (plugin.devMode)
                          _Badge(
                            text: s.pluginDevModeBadge,
                            color: c.accent,
                            bg: m.subtle(c.accent),
                          ),
                        _Badge(
                          text: loadFailed
                              ? s.pluginLoadStatusFailed
                              : s.pluginLoadStatusLoaded,
                          color: loadFailed ? AppColors.red : AppColors.green,
                          bg: m.subtle(
                            loadFailed ? AppColors.red : AppColors.green,
                          ),
                        ),
                        if (plugin.disabledReason == 'Manual')
                          _Badge(
                            text: s.pluginDisabledManual,
                            color: c.textSecondary,
                            bg: c.surface2,
                          ),
                        if (plugin.disabledReason == 'CircuitBreaker')
                          _Badge(
                            text: s.pluginDisabledCircuitBreaker,
                            color: AppColors.red,
                            bg: m.subtle(AppColors.red),
                          ),
                      ],
                    ),
                    if (plugin.description.isNotEmpty) ...[
                      const SizedBox(height: 2),
                      _HoverDescription(text: plugin.description),
                    ],
                    if (loadFailed && plugin.loadError.isNotEmpty)
                      Padding(
                        padding: const EdgeInsets.only(top: 4),
                        child: GestureDetector(
                          onTap: () => _showLoadError(context),
                          child: Text(
                            plugin.loadError,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 11,
                              color: AppColors.red,
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
              const SizedBox(width: 12),
              ShadSwitch(
                enabled: !loadFailed,
                value: plugin.enabled && !loadFailed,
                onChanged: loadFailed
                    ? null
                    : (v) => provider.setEnabled(plugin.identity, v),
              ),
              const SizedBox(width: 4),
              if (!loadFailed && plugin.settings.isNotEmpty)
                ShadIconButton.ghost(
                  icon: Icon(
                    LucideIcons.settings2,
                    size: 16,
                    color: c.textSecondary,
                  ),
                  onPressed: () => showPluginSettingsDialog(
                    context,
                    plugin: plugin,
                    provider: provider,
                  ),
                ),
              if (!loadFailed && plugin.authSupported)
                ShadTooltip(
                  effects: const [],
                  builder: (_) => Text(s.pluginAuthButton),
                  child: ShadIconButton.ghost(
                    icon: Icon(
                      LucideIcons.logIn,
                      size: 16,
                      color: c.textSecondary,
                    ),
                    onPressed: onAuth,
                  ),
                ),
              ShadIconButton.ghost(
                icon: Icon(LucideIcons.trash2, size: 16, color: AppColors.red),
                onPressed: onUninstall,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _PluginAuthDialog extends StatefulWidget {
  final PluginInfoSignal plugin;
  final PluginProvider provider;

  const _PluginAuthDialog({required this.plugin, required this.provider});

  @override
  State<_PluginAuthDialog> createState() => _PluginAuthDialogState();
}

class _PluginAuthDialogState extends State<_PluginAuthDialog> {
  int _lastSeq = -1;
  String _sessionId = '';
  String _authRef = '';
  String _site = '';
  String _input = '';
  String _status = '';
  String _challenge = '';
  String _challengeType = '';
  String _message = '';
  String _lastAction = '';
  // 对话框打开即查询登录态，首帧显示 loading 而非按钮。
  bool _busy = true;
  // 当前在途请求是否来自后台自动轮询：轮询不得锁定输入框/操作按钮。
  bool _autoPoll = false;
  bool _cancelSent = false;
  Timer? _pollTimer;
  Timer? _siteStatusTimer;

  /// 由用户主动动作（begin/poll 点击、logout、切换站点）触发、仍在等待引擎
  /// 回包的状态；后台自动轮询不计入，避免每 2 秒把输入框/按钮锁一次。
  bool get _controlsBusy => _busy && !_autoPoll;

  @override
  void initState() {
    super.initState();
    _lastSeq = widget.provider.authResultSeq;
    widget.provider.addListener(_onProviderChanged);
    WidgetsBinding.instance.addPostFrameCallback((_) => _refreshSavedAuth());
  }

  @override
  void dispose() {
    widget.provider.removeListener(_onProviderChanged);
    _pollTimer?.cancel();
    _siteStatusTimer?.cancel();
    // Esc / 遮罩 / 关闭按钮都走这里：还有会话挂着就通知引擎释放，
    // 避免插件侧的轮询状态一直挂到超时（显式「取消」按钮已在 _cancel 里发过，
    // _cancelSent 去重，这里不会重复发送）。
    _sendCancelIfPending();
    super.dispose();
  }

  void _onProviderChanged() {
    if (!mounted || widget.provider.authResultSeq == _lastSeq) return;
    _lastSeq = widget.provider.authResultSeq;
    final result = widget.provider.lastAuthResult;
    if (result == null || result.identity != widget.plugin.identity) return;
    // 会话已经切换（取消旧会话后立即发起新一轮）：丢弃旧会话的迟到响应，
    // 避免多插件/多会话信号串扰。
    if (_sessionId.isNotEmpty &&
        result.sessionId.isNotEmpty &&
        result.sessionId != _sessionId) {
      return;
    }
    final wasLogout = _lastAction == 'logout';
    setState(() {
      _busy = false;
      _autoPoll = false;
      _status = result.status;
      _message = result.message;
      _cancelSent = false;
      if (wasLogout) {
        // 引擎侧无论 logout 成功与否都已删除档案：本地登录态一并清空，
        // 否则 authRef 残留会让「注销」按钮继续显示，用户回不到登录态。
        _authRef = '';
        _sessionId = '';
        _challenge = '';
        _challengeType = '';
      } else {
        _sessionId = result.status == 'pending' ? result.sessionId : '';
        _authRef = result.status == 'success' ? result.authRef : '';
        // challenge/challengeType 在协议上是可选字段：pending 轮询回包省略
        // 时保留上一帧，避免二维码在第一次 poll 后消失。
        if (result.status == 'pending') {
          if (result.challenge.isNotEmpty) _challenge = result.challenge;
          if (result.challengeType.isNotEmpty) {
            _challengeType = result.challengeType;
          }
        } else {
          _challenge = result.challenge;
          _challengeType = result.challengeType;
        }
      }
    });
    if (_status == 'pending' && _challengeType.toLowerCase() == 'qrcode') {
      _startPolling();
    } else {
      _pollTimer?.cancel();
    }
  }

  void _refreshSavedAuth() {
    if (!mounted) return;
    _lastAction = 'status';
    setState(() => _busy = true);
    widget.provider.authenticate(
      identity: widget.plugin.identity,
      action: 'status',
      site: _site,
    );
  }

  void _onSiteChanged(String value) {
    _site = value;
    if (_sessionId.isNotEmpty) {
      _sendCancelIfPending();
      _pollTimer?.cancel();
      setState(() {
        _sessionId = '';
        _challenge = '';
        _challengeType = '';
        _status = '';
      });
    }
    _siteStatusTimer?.cancel();
    _siteStatusTimer = Timer(
      const Duration(milliseconds: 350),
      _refreshSavedAuth,
    );
  }

  void _startPolling() {
    _pollTimer?.cancel();
    _pollTimer = Timer.periodic(const Duration(seconds: 2), (_) {
      if (!mounted || _busy || _sessionId.isEmpty) return;
      _submit(auto: true);
    });
  }

  void _submit({bool auto = false}) {
    if (_busy) return;
    _lastAction = _sessionId.isEmpty ? 'begin' : 'poll';
    setState(() {
      _busy = true;
      _autoPoll = auto;
    });
    widget.provider.authenticate(
      identity: widget.plugin.identity,
      action: _lastAction,
      site: _site,
      authRef: _authRef,
      sessionId: _sessionId,
      input: _input,
    );
  }

  void _logout() {
    if (_busy) return;
    _lastAction = 'logout';
    setState(() => _busy = true);
    _pollTimer?.cancel();
    widget.provider.authenticate(
      identity: widget.plugin.identity,
      action: 'logout',
      site: _site,
      authRef: _authRef,
    );
  }

  /// 对当前 pending 会话发一次 cancel（不等待回包）；同一会话只发一次。
  void _sendCancelIfPending() {
    if (_sessionId.isEmpty || _cancelSent) return;
    _cancelSent = true;
    widget.provider.authenticate(
      identity: widget.plugin.identity,
      action: 'cancel',
      site: _site,
      authRef: _authRef,
      sessionId: _sessionId,
    );
  }

  void _cancel() {
    _sendCancelIfPending();
    Navigator.of(context).pop();
  }

  Widget _challengeWidget() {
    if (_challenge.isEmpty) return const SizedBox.shrink();
    if (_challenge.startsWith('data:image/')) {
      final comma = _challenge.indexOf(',');
      if (comma > 0) {
        try {
          final bytes = base64Decode(_challenge.substring(comma + 1));
          return Image.memory(
            bytes,
            height: 220,
            width: 220,
            fit: BoxFit.contain,
          );
        } on FormatException catch (_) {
          // Fall through to text representation.
        }
      }
    }
    if (_challengeType.toLowerCase() == 'qrcode') {
      return QrImageView(
        data: _challenge,
        size: 240,
        backgroundColor: const Color(0xffffffff),
        padding: const EdgeInsets.all(10),
      );
    }
    return Text(_challenge);
  }

  Color _statusColor(AppColors c) {
    switch (_status) {
      case 'success':
        return AppColors.green;
      case 'error':
        return AppColors.red;
      default:
        return c.textSecondary;
    }
  }

  String _statusLabel(S s) {
    switch (_status) {
      case 'pending':
        return s.pluginAuthPending;
      case 'success':
        return s.pluginAuthSuccess;
      case 'error':
        return s.pluginAuthFailed(
          _message.isNotEmpty ? _message : s.pluginAuthInvalidResponse,
        );
      case '':
        return '';
      default:
        // 插件返回了 begin/poll/cancel/logout/status 之外的未知 status。
        return s.pluginAuthInvalidResponse;
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = currentS;
    final c = AppColors.of(context);
    final label = _statusLabel(s);
    final supplement =
        (_status == 'pending' || _status == 'success') && _message.isNotEmpty
        ? _message
        : '';
    return ShadDialog(
      title: Text(
        _challengeType.toLowerCase() == 'qrcode'
            ? s.pluginAuthQr
            : s.pluginAuthDialogTitle(widget.plugin.name),
      ),
      description: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(s.pluginAuthDescription),
            const SizedBox(height: 8),
            ShadInput(
              placeholder: Text(s.pluginAuthSitePlaceholder),
              enabled: !_controlsBusy,
              onChanged: _onSiteChanged,
            ),
            const SizedBox(height: 8),
            ShadInput(
              placeholder: Text(s.pluginAuthInputPlaceholder),
              enabled: !_controlsBusy,
              onChanged: (v) => _input = v,
            ),
            if (_busy && !_autoPoll && _challenge.isEmpty) ...[
              const SizedBox(height: 10),
              Text(
                s.pluginCommonLoading,
                style: TextStyle(color: c.textSecondary),
              ),
            ],
            if (_challenge.isNotEmpty) ...[
              const SizedBox(height: 10),
              Container(
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(
                  color: c.surface2,
                  borderRadius: AppMetrics.of(context).brDialog,
                ),
                child: Column(children: [_challengeWidget()]),
              ),
            ],
            if (label.isNotEmpty) ...[
              const SizedBox(height: 8),
              Text(label, style: TextStyle(color: _statusColor(c))),
            ],
            if (supplement.isNotEmpty) ...[
              const SizedBox(height: 4),
              Text(
                supplement,
                style: TextStyle(color: c.textSecondary, fontSize: 11),
              ),
            ],
          ],
        ),
      ),
      actions: [
        if (_authRef.isEmpty)
          ShadButton(
            onPressed: _controlsBusy ? null : _submit,
            child: Text(
              _sessionId.isEmpty ? s.pluginAuthBegin : s.pluginAuthPoll,
            ),
          ),
        if (_sessionId.isNotEmpty)
          ShadButton.outline(
            onPressed: _controlsBusy ? null : _cancel,
            child: Text(s.cancel),
          ),
        if (_authRef.isNotEmpty && _status == 'success')
          ShadButton.outline(
            onPressed: _controlsBusy ? null : _logout,
            child: Text(s.pluginAuthLogout),
          ),
      ],
    );
  }
}

// =============================================================================
// 市场条目卡片
// =============================================================================

class _MarketCard extends StatefulWidget {
  final MarketEntrySignal entry;
  final bool installed;
  final PluginProvider provider;

  const _MarketCard({
    required this.entry,
    required this.installed,
    required this.provider,
  });

  @override
  State<_MarketCard> createState() => _MarketCardState();
}

class _MarketCardState extends State<_MarketCard> {
  bool _pending = false;
  int _lastSeenOpSeq = -1;

  @override
  void initState() {
    super.initState();
    _lastSeenOpSeq = widget.provider.opResultSeq;
  }

  @override
  void didUpdateWidget(covariant _MarketCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    final seq = widget.provider.opResultSeq;
    if (seq != _lastSeenOpSeq) {
      _lastSeenOpSeq = seq;
      if (widget.provider.lastOpResult?.op == 'market_install') {
        _pending = false;
      }
    }
    if (widget.installed) _pending = false;
  }

  void _install() {
    setState(() => _pending = true);
    widget.provider.installMarket(widget.entry.pluginId);
  }

  @override
  Widget build(BuildContext context) {
    final s = currentS;
    final c = AppColors.of(context);
    final m = AppMetrics.of(context);
    final entry = widget.entry;
    final busy = _pending && !widget.installed;
    final yankedLabel = switch (entry.yanked) {
      'deprecated' => s.marketYankedDeprecated,
      'vulnerable' => s.marketYankedVulnerable,
      'malicious' => s.marketYankedMalicious,
      _ => null,
    };

    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: () => showPluginDetailDialog(
        context,
        name: entry.name,
        version: entry.version,
        identity: entry.pluginId,
        description: entry.description,
        homepage: entry.homepage,
        author: entry.author,
        tags: entry.tags,
        publishTime: entry.publishTime,
        minAppVersion: entry.minAppVersion,
        yankedLabel: yankedLabel,
        permissions: entry.permissions,
      ),
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          decoration: BoxDecoration(
            color: c.surface1,
            borderRadius: m.brDialog,
            border: Border.all(color: c.border, width: 1),
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Wrap(
                      crossAxisAlignment: WrapCrossAlignment.center,
                      spacing: 8,
                      runSpacing: 4,
                      children: [
                        Text(
                          entry.name.isNotEmpty ? entry.name : entry.pluginId,
                          style: TextStyle(
                            fontSize: 13.5,
                            fontWeight: FontWeight.w600,
                            color: c.textPrimary,
                          ),
                        ),
                        Text(
                          'v${entry.version}',
                          style: TextStyle(fontSize: 11, color: c.textMuted),
                        ),
                        if (entry.author.isNotEmpty)
                          Text(
                            entry.author,
                            style: TextStyle(fontSize: 11, color: c.textMuted),
                          ),
                        if (entry.homepage.isNotEmpty)
                          GestureDetector(
                            onTap: () => launchUrl(Uri.parse(entry.homepage)),
                            child: Text(
                              entry.homepage,
                              style: TextStyle(fontSize: 11, color: c.accent),
                            ),
                          ),
                        if (yankedLabel != null)
                          _Badge(
                            text: yankedLabel,
                            color: AppColors.red,
                            bg: m.subtle(AppColors.red),
                          ),
                      ],
                    ),
                    if (entry.description.isNotEmpty) ...[
                      const SizedBox(height: 2),
                      _HoverDescription(text: entry.description),
                    ],
                  ],
                ),
              ),
              const SizedBox(width: 12),
              ShadButton.outline(
                onPressed: (widget.installed || busy) ? null : _install,
                child: Text(
                  widget.installed
                      ? s.marketInstalledButton
                      : busy
                      ? s.marketInstallingButton
                      : s.marketInstallButton,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

// =============================================================================
// 单行省略描述：鼠标悬浮 tooltip 显示全文
// =============================================================================

class _HoverDescription extends StatelessWidget {
  final String text;

  const _HoverDescription({required this.text});

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    return ShadTooltip(
      waitDuration: const Duration(milliseconds: 300),
      builder: (_) => ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 380),
        child: Text(text),
      ),
      child: Text(
        text,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(fontSize: 12, color: c.textSecondary),
      ),
    );
  }
}

// =============================================================================
// 小徽章
// =============================================================================

class _Badge extends StatelessWidget {
  final String text;
  final Color color;
  final Color bg;

  const _Badge({required this.text, required this.color, required this.bg});

  @override
  Widget build(BuildContext context) {
    final m = AppMetrics.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
      decoration: BoxDecoration(color: bg, borderRadius: m.brPill),
      child: Text(
        text,
        style: TextStyle(
          fontSize: 10.5,
          fontWeight: FontWeight.w500,
          color: color,
        ),
      ),
    );
  }
}
