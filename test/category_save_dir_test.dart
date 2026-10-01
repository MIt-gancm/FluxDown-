// 回归测试：浏览器扩展右键下载未按文件分类归入对应保存目录。
//
// 根因：SettingsProvider.resolveCategorySaveDir(fileName, {url}) 依赖 fileName
// 做扩展名匹配；扩展右键下载等场景常常只有 URL、没有已解析的文件名（或文件名
// 不含扩展名）。修复：fileName 为空或不含 '.' 时，改用 URL 路径末段派生文件名
// 参与匹配。
//
// 覆盖：
//   A. CustomCategory.matches 的匹配契约（扩展名/正则/大小写/all/other）；
//   B. 真实 SettingsProvider.resolveCategorySaveDir 的分类选择与 URL 派生，
//      经 applyLoadedConfig 注入分类配置（带 program_category_migrated 标记，
//      避免触发需要 Rust 引擎的迁移写入）。

import 'package:flutter_test/flutter_test.dart';
import 'package:flux_down/src/bindings/bindings.dart';
import 'package:flux_down/src/models/custom_category.dart';
import 'package:flux_down/src/models/settings_provider.dart';

/// 以 [categories] 作为已加载配置构造真实 SettingsProvider。
SettingsProvider _providerWith(List<CustomCategory> categories) {
  final settings = SettingsProvider();
  addTearDown(settings.dispose);
  settings.applyLoadedConfig([
    ConfigEntry(
      key: 'custom_categories',
      value: CustomCategory.encodeList(categories),
    ),
    ConfigEntry(key: 'program_category_migrated', value: 'true'),
  ]);
  return settings;
}

void main() {
  group('CustomCategory.matches — 真实生产代码，验证 URL 派生文件名的匹配契约', () {
    final archive = CustomCategory.defaultCategories().firstWhere(
      (c) => c.builtinType == 'archive',
    );
    final video = CustomCategory.defaultCategories().firstWhere(
      (c) => c.builtinType == 'video',
    );
    final other = CustomCategory.defaultCategories().firstWhere(
      (c) => c.builtinType == 'other',
    );
    final all = CustomCategory.defaultCategories().firstWhere(
      (c) => c.builtinType == 'all',
    );

    test('压缩包分类命中 URL 派生的 .zip / .tar.gz 文件名', () {
      expect(archive.matches('file.zip'), isTrue);
      expect(archive.matches('a/b/c.tar.gz'.split('/').last), isTrue);
    });

    test('扩展名匹配忽略大小写', () {
      expect(archive.matches('FILE.ZIP'), isTrue);
    });

    test('视频分类命中 mp4，但压缩包分类不命中同一文件名', () {
      expect(video.matches('movie.mp4'), isTrue);
      expect(archive.matches('movie.mp4'), isFalse);
    });

    test('无扩展名或以点结尾的文件名不命中任何扩展名分类', () {
      expect(archive.matches('noext'), isFalse);
      expect(archive.matches('trailingdot.'), isFalse);
      expect(archive.matches(''), isFalse);
    });

    test("builtinType == 'other' 永不通过 matches 命中（由调用方专门处理排除逻辑）", () {
      expect(other.matches('anything.zip'), isFalse);
      expect(other.matches(''), isFalse);
    });

    test("builtinType == 'all' 命中任意文件名", () {
      expect(all.matches('movie.mp4'), isTrue);
      expect(all.matches('noext'), isTrue);
      expect(all.matches(''), isTrue);
    });

    test('正则模式：自定义分类按 regexPattern 匹配文件名', () {
      const screenshot = CustomCategory(
        id: 'c1',
        name: 'screenshots',
        matchMode: MatchMode.regex,
        regexPattern: r'^screenshot.*\.png$',
        saveDir: '/tmp/screenshots',
      );
      expect(screenshot.matches('screenshot_2024.png'), isTrue);
      expect(screenshot.matches('SCREENSHOT_final.png'), isTrue); // 大小写不敏感
      expect(screenshot.matches('photo.png'), isFalse);
    });
  });

  group('resolveCategorySaveDir — 真实 SettingsProvider', () {
    List<CustomCategory> categoriesWithSaveDirs({
      String archiveDir = '',
      String videoDir = '',
      String documentDir = '',
      String otherDir = '',
    }) {
      return CustomCategory.defaultCategories().map((c) {
        switch (c.builtinType) {
          case 'archive':
            return c.copyWith(saveDir: archiveDir);
          case 'video':
            return c.copyWith(saveDir: videoDir);
          case 'document':
            return c.copyWith(saveDir: documentDir);
          case 'other':
            return c.copyWith(saveDir: otherDir);
          default:
            return c;
        }
      }).toList();
    }

    String resolve(
      List<CustomCategory> categories,
      String fileName, {
      String url = '',
    }) => _providerWith(categories).resolveCategorySaveDir(fileName, url: url);

    test('(a) fileName 为空 + url 带 .zip → 命中压缩包分类 saveDir', () {
      final categories = categoriesWithSaveDirs(archiveDir: '/downloads/zip');
      expect(
        resolve(categories, '', url: 'https://cdn.example.com/pack.zip'),
        '/downloads/zip',
      );
    });

    test('(b) fileName 无扩展名 + url 带 .mp4 → 命中视频分类 saveDir', () {
      final categories = categoriesWithSaveDirs(videoDir: '/downloads/video');
      expect(
        resolve(categories, 'blob', url: 'https://cdn.example.com/movie.mp4'),
        '/downloads/video',
      );
    });

    test('(c) fileName 已带扩展名时以 fileName 为准，忽略 url', () {
      final categories = categoriesWithSaveDirs(
        videoDir: '/downloads/video',
        documentDir: '/downloads/doc',
      );
      expect(
        resolve(
          categories,
          'report.docx',
          url: 'https://cdn.example.com/movie.mp4',
        ),
        '/downloads/doc',
      );
    });

    test('(c2) fileName 带未知扩展名时，即使 url 能命中也不回退到 url', () {
      final categories = categoriesWithSaveDirs(videoDir: '/downloads/video');
      expect(
        resolve(
          categories,
          'data.xyz',
          url: 'https://cdn.example.com/movie.mp4',
        ),
        '',
      );
    });

    test('(d) 均不命中普通分类 + other 未设置 saveDir → 返回空字符串', () {
      expect(
        resolve(
          categoriesWithSaveDirs(),
          '',
          url: 'https://cdn.example.com/data.xyz',
        ),
        '',
      );
    });

    test('(e) 不命中普通分类，但 other 设置了 saveDir → 回退到 other', () {
      final categories = categoriesWithSaveDirs(otherDir: '/downloads/misc');
      expect(
        resolve(categories, '', url: 'https://cdn.example.com/data.xyz'),
        '/downloads/misc',
      );
    });

    test('普通分类命中但未配置 saveDir 时，不会误回退到 other', () {
      final categories = categoriesWithSaveDirs(otherDir: '/downloads/misc');
      expect(
        resolve(categories, '', url: 'https://cdn.example.com/movie.mp4'),
        '',
      );
    });

    test('fileName 与 url 都为空时直接返回空字符串', () {
      final categories = categoriesWithSaveDirs(otherDir: '/downloads/misc');
      expect(resolve(categories, ''), '');
    });

    group('URL 派生文件名', () {
      late List<CustomCategory> categories;
      setUp(() {
        categories = categoriesWithSaveDirs(
          archiveDir: '/downloads/zip',
          videoDir: '/downloads/video',
        );
      });

      test('忽略 query string', () {
        expect(
          resolve(
            categories,
            '',
            url: 'https://cdn.example.com/movie.mp4?token=abc&x=1',
          ),
          '/downloads/video',
        );
      });

      test('末段含多个点时保留完整文件名（.tar.gz）', () {
        expect(
          resolve(categories, '', url: 'https://cdn.example.com/a/b/c.tar.gz'),
          '/downloads/zip',
        );
      });

      test('末段不含 "." 时无法归类', () {
        expect(
          resolve(categories, '', url: 'https://cdn.example.com/download'),
          '',
        );
      });

      test('URL 无法解析时不抛异常并返回空', () {
        expect(resolve(categories, '', url: 'not a valid url ::: %%%'), '');
      });
    });
  });
}
