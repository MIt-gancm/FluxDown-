/** 预览窗口的静态演示数据(任务列表 + 分段),与旧版构建器保持一致。 */
import type { FluxThemeJson } from "@/lib/theme-builder";

export interface PreviewSegment {
  index: number;
  startByte: number;
  endByte: number;
  downloadedBytes: number;
}

export type TaskStatus = "downloading" | "paused" | "completed" | "error";
export type FileCategory = "all" | "video" | "audio" | "document" | "image" | "archive" | "other";
export type StatusFilter = "all" | TaskStatus;

export interface PreviewTask {
  id: string;
  ext: string;
  name: string;
  size: string;
  totalBytes: number;
  downloadedBytes: number;
  progress: number;
  speed: string;
  status: TaskStatus;
  fileCategory: FileCategory;
  segments: PreviewSegment[];
  url: string;
  saveDir: string;
  eta: string;
}

const VIDEO_TOTAL_BYTES = 2_254_857_830;

export const PREVIEW_TASKS: PreviewTask[] = [
  {
    id: "t1",
    ext: "zip",
    name: "4K-wallpaper-collection.zip",
    size: "847.2 MB",
    totalBytes: 888_300_000,
    downloadedBytes: 597_823_900,
    progress: 67.3,
    speed: "---",
    status: "paused",
    fileCategory: "archive",
    segments: [
      { index: 0, startByte: 0, endByte: 222_074_999, downloadedBytes: 222_074_999 },
      { index: 1, startByte: 222_075_000, endByte: 444_149_999, downloadedBytes: 195_000_000 },
      { index: 2, startByte: 444_150_000, endByte: 666_224_999, downloadedBytes: 120_748_900 },
      { index: 3, startByte: 666_225_000, endByte: 888_299_999, downloadedBytes: 60_000_000 },
    ],
    url: "https://cdn.example.com/4K-wallpaper-collection.zip",
    saveDir: "D:\\Downloads",
    eta: "---",
  },
  {
    id: "t2",
    ext: "mp4",
    name: "React-Advanced-Tutorial.mp4",
    size: "2.1 GB",
    totalBytes: VIDEO_TOTAL_BYTES,
    downloadedBytes: 1_657_320_506,
    progress: 73.5,
    speed: "45.2 MB/s",
    status: "downloading",
    fileCategory: "video",
    segments: [
      { index: 0, startByte: 0, endByte: 281_857_228, downloadedBytes: 281_857_228 },
      { index: 1, startByte: 281_857_229, endByte: 563_714_457, downloadedBytes: 281_857_228 },
      { index: 2, startByte: 563_714_458, endByte: 845_571_686, downloadedBytes: 281_857_228 },
      { index: 3, startByte: 845_571_687, endByte: 1_127_428_915, downloadedBytes: 245_000_000 },
      { index: 4, startByte: 1_127_428_916, endByte: 1_409_286_144, downloadedBytes: 200_000_000 },
      { index: 5, startByte: 1_409_286_145, endByte: 1_691_143_373, downloadedBytes: 180_000_000 },
      { index: 6, startByte: 1_691_143_374, endByte: 1_973_000_602, downloadedBytes: 120_000_000 },
      { index: 7, startByte: 1_973_000_603, endByte: VIDEO_TOTAL_BYTES, downloadedBytes: 66_748_822 },
    ],
    url: "https://media.example.com/React-Advanced-Tutorial.mp4",
    saveDir: "D:\\Downloads",
    eta: "13s",
  },
  {
    id: "t3",
    ext: "pdf",
    name: "annual-report-2025.pdf",
    size: "24.6 MB",
    totalBytes: 25_795_276,
    downloadedBytes: 25_795_276,
    progress: 100,
    speed: "---",
    status: "completed",
    fileCategory: "document",
    segments: [
      { index: 0, startByte: 0, endByte: 12_897_637, downloadedBytes: 12_897_637 },
      { index: 1, startByte: 12_897_638, endByte: 25_795_275, downloadedBytes: 12_897_638 },
    ],
    url: "https://reports.example.com/annual-report-2025.pdf",
    saveDir: "D:\\Downloads",
    eta: "---",
  },
  {
    id: "t4",
    ext: "gz",
    name: "project-v2.0-src.tar.gz",
    size: "312.4 MB",
    totalBytes: 327_580_000,
    downloadedBytes: 147_738_580,
    progress: 45.1,
    speed: "28.7 MB/s",
    status: "downloading",
    fileCategory: "archive",
    segments: [
      { index: 0, startByte: 0, endByte: 81_894_999, downloadedBytes: 81_895_000 },
      { index: 1, startByte: 81_895_000, endByte: 163_789_999, downloadedBytes: 45_000_000 },
      { index: 2, startByte: 163_790_000, endByte: 245_684_999, downloadedBytes: 15_843_580 },
      { index: 3, startByte: 245_685_000, endByte: 327_579_999, downloadedBytes: 5_000_000 },
    ],
    url: "https://releases.example.com/project-v2.0-src.tar.gz",
    saveDir: "D:\\Downloads",
    eta: "6s",
  },
  {
    id: "t5",
    ext: "exe",
    name: "system-driver-update.exe",
    size: "89.3 MB",
    totalBytes: 93_633_536,
    downloadedBytes: 11_236_024,
    progress: 12,
    speed: "---",
    status: "error",
    fileCategory: "other",
    segments: [
      { index: 0, startByte: 0, endByte: 46_816_767, downloadedBytes: 11_236_024 },
      { index: 1, startByte: 46_816_768, endByte: 93_633_535, downloadedBytes: 0 },
    ],
    url: "https://drivers.example.com/system-driver-update.exe",
    saveDir: "D:\\Downloads",
    eta: "---",
  },
];

export const FILE_CATEGORIES: FileCategory[] = ["all", "video", "audio", "document", "image", "archive", "other"];
export const STATUS_FILTERS: StatusFilter[] = ["all", "downloading", "completed", "error", "paused"];

/** 分段分布网格:44×9 格,每格按其字节中点是否已下载着色(暗色下略微强化可见性)。 */
export function buildGridCells(theme: FluxThemeJson, segments: PreviewSegment[], totalBytes: number): boolean[] {
  const totalCells = 44 * 9;
  const filled = new Array<boolean>(totalCells).fill(false);
  for (let i = 0; i < totalCells; i += 1) {
    const start = Math.floor((totalBytes * i) / totalCells);
    const end = Math.floor((totalBytes * (i + 1)) / totalCells);
    const seg = segments.find((item) => start < item.endByte && end > item.startByte);
    if (!seg) continue;
    const segSize = seg.endByte - seg.startByte + 1;
    const segProgress = Math.min(1, seg.downloadedBytes / segSize);
    const localMid = start + (end - start) / 2 - seg.startByte;
    filled[i] = localMid / segSize <= segProgress;
  }
  if (theme.appearance === "light") return filled;
  return filled.map((v, idx) => (idx % 11 === 0 ? true : v));
}
