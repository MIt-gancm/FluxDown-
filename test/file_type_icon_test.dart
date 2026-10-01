import 'package:flutter_test/flutter_test.dart';
import 'package:flux_down/src/models/download_task.dart';
import 'package:flux_down/src/widgets/file_type_icon.dart';
import 'package:shadcn_ui/shadcn_ui.dart';

void main() {
  group('fileTypeIcon', () {
    test('精确表优先于分类回落', () {
      // iso 属 archive，但光盘字形比通用压缩包更达意
      expect(fileTypeIcon('iso'), LucideIcons.disc);
      expect(fileTypeIcon('zip'), LucideIcons.archive);
      // 三种安装包同属 program，字形必须各不相同
      expect(fileTypeIcon('exe'), LucideIcons.appWindow);
      expect(fileTypeIcon('apk'), LucideIcons.smartphone);
      expect(fileTypeIcon('deb'), LucideIcons.package);
      expect(fileTypeIcon('torrent'), LucideIcons.magnet);
    });

    test('ts 是 MPEG-TS 视频切片，不得被当作 TypeScript 源码', () {
      expect(fileTypeIcon('ts'), LucideIcons.film);
      expect(fileTypeIcon('tsx'), LucideIcons.fileCode);
    });

    test('未命中精确表时回落分类字形', () {
      expect(fileTypeIcon('mkv'), fileCategoryIcon(FileCategory.video));
      expect(fileTypeIcon('flac'), fileCategoryIcon(FileCategory.audio));
      expect(fileTypeIcon('png'), fileCategoryIcon(FileCategory.image));
      expect(fileTypeIcon('txt'), fileCategoryIcon(FileCategory.document));
    });

    test('大小写不敏感；无扩展名落到通用文件字形', () {
      expect(fileTypeIcon('EXE'), fileTypeIcon('exe'));
      expect(fileTypeIcon('MkV'), fileTypeIcon('mkv'));
      // DownloadTask.fileExtension 对无扩展名的文件返回 '?'
      expect(fileTypeIcon('?'), LucideIcons.file);
      expect(fileTypeIcon('zzzz'), LucideIcons.file);
    });
  });
}
