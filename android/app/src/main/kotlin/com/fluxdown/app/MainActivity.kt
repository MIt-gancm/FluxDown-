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
 * 兼容老调用方：某些应用/浏览器用显式 intent 硬编码调起本入口并携带下载数据
 * （ACTION_SEND / VIEW 的 http/https/magnet/ed2k/fluxdown 直链）。此时把原
 * intent 原样转发给 [ExternalDownloadActivity]（透明弹新建下载框），结束自身，
 * 让成熟的下载弹窗流程接管——老版本调用方无需改动即可走外部下载。
 *
 * channel 优先在 [configureFlutterEngine] 中绑定，确保 Dart entrypoint 执行前即可响应；
 * [onStart] 对旧 embedding 或 cached engine 的差异提供幂等兜底。
 */
class MainActivity : FlutterActivity() {
    override fun getCachedEngineId(): String? =
        if (FluxdownEngine.cached != null) FluxdownEngine.ENGINE_ID else null

    override fun shouldDestroyEngineWithHost(): Boolean = false

    /** 下载请求只转发一次（onCreate/onStart/onNewIntent 多处可触发，防重复弹窗）。 */
    private var downloadForwarded = false

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
        forwardIfDownloadRequest(intent)
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
        forwardIfDownloadRequest(intent)
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

    /** 热启动（singleTask）：应用已在任务栈中被前台化，新 intent 到达。 */
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        forwardIfDownloadRequest(intent)
    }

    /** 携带下载参数的外部调用（区别于纯 MAIN/LAUNCHER 桌面启动）→ 转发并结束自身。 */
    private fun forwardIfDownloadRequest(intent: Intent) {
        if (downloadForwarded || !isDownloadRequest(intent)) return
        downloadForwarded = true
        startActivity(
            Intent(intent).apply {
                setClass(this@MainActivity, ExternalDownloadActivity::class.java)
            },
        )
        finish()
    }

    /** 是否带可下载载荷：SEND 有分享文本；VIEW 的 data scheme 属下载直链。 */
    private fun isDownloadRequest(intent: Intent): Boolean = when (intent.action) {
        Intent.ACTION_SEND, Intent.ACTION_SEND_MULTIPLE ->
            !intent.getStringExtra(Intent.EXTRA_TEXT).isNullOrBlank()
        Intent.ACTION_VIEW -> intent.data?.scheme?.lowercase() in DOWNLOAD_SCHEMES
        else -> false
    }

    private companion object {
        val DOWNLOAD_SCHEMES =
            setOf("http", "https", "magnet", "ed2k", "fluxdown")
    }
}
