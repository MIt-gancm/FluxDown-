package com.fluxdown.app

import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.FlutterEngineCache

/**
 * FluxDown 单 FlutterEngine 保持者。
 *
 * MainActivity 与 ExternalDownloadActivity 共享同一个引擎，保证单 Dart 会话、
 * 下载状态、Rust 桥与前台服务不重复初始化。首个启动的 Activity 走 FlutterActivity
 * 默认的 createFlutterEngine 路径（插件在此注册一次、Dart entrypoint 只运行一次），
 * 并在其 onStart 里经 [cacheIfAbsent] 把引擎缓存；后续 Activity 通过 override
 * [io.flutter.embedding.android.FlutterActivity.getCachedEngineId] 复用引擎。
 * 新 embedding 中引擎为进程级持有，不随 Activity 销毁自动销毁，故复用方始终
 * 拿到同一个仍存活的引擎。缓存/绑定统一放在 onStart，避免受 configureFlutterEngine
 * 对 cached 引擎是否触发的版本差异影响。
 */
object FluxdownEngine {
    const val ENGINE_ID = "com.fluxdown.app/engine"

    val cached: FlutterEngine?
        get() = FlutterEngineCache.getInstance().get(ENGINE_ID)

    fun cacheIfAbsent(engine: FlutterEngine) {
        if (cached == null) {
            FlutterEngineCache.getInstance().put(ENGINE_ID, engine)
        }
    }
}