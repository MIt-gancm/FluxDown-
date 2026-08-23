package com.fluxdown.app

import android.content.Intent
import android.os.Bundle
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine

/**
 * FluxDown 移动端启动入口 + 本地存储桥宿主。
 *
 * 只承担应用 Launcher（不透明主题，见 MainTheme）与 [AppStorage] 本地存储能力；
 * 外部下载唤起由 [ExternalDownloadActivity] 承载（透明窗口弹下载框）。二者共享
 * 同一个 FlutterEngine（见 [FluxdownEngine]），保持单 Dart 会话、下载状态、
 * Rust 桥与前台服务不重复初始化。
 *
 * channel 优先在 [configureFlutterEngine] 中绑定，确保 Dart entrypoint 执行前即可响应；
 * [onStart] 对旧 embedding 或 cached engine 的差异提供幂等兜底。
 */
class MainActivity : FlutterActivity() {
    override fun getCachedEngineId(): String? =
        if (FluxdownEngine.cached != null) FluxdownEngine.ENGINE_ID else null

    override fun shouldDestroyEngineWithHost(): Boolean = false

    override fun onCreate(savedInstanceState: Bundle?) {
        // 桌面图标再进：部分 ROM 会在已有任务上再叠一个 MAIN/LAUNCHER
        // Activity，从而尝试重复挂载共享引擎。
        if (!isTaskRoot &&
            intent.hasCategory(Intent.CATEGORY_LAUNCHER) &&
            intent.action == Intent.ACTION_MAIN
        ) {
            finish()
            return
        }
        super.onCreate(savedInstanceState)
    }

    override fun detachFromFlutterEngine() {
        super.detachFromFlutterEngine()
        finish()
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        FluxdownEngine.cacheIfAbsent(flutterEngine)
        AppStorage.bind(flutterEngine, this)
    }

    override fun onStart() {
        super.onStart()
        getFlutterEngine()?.let { engine ->
            FluxdownEngine.cacheIfAbsent(engine)
            AppStorage.bind(engine, this)
        }
    }

    override fun onDestroy() {
        AppStorage.unbind(this)
        super.onDestroy()
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        // 目录选择器结果由 AppStorage 处理；其余交给默认行为。
        if (!AppStorage.onActivityResult(this, requestCode, resultCode, data)) {
            super.onActivityResult(requestCode, resultCode, data)
        }
    }
}