/**
 * GPUI 客户端复刻界面的演示数据(全部为开源/公开发行物,文件名与体积为示意)。
 * SSR 首帧与脚本动画共用这一份,保证无 JS 时静态画面与动画首帧一致。
 */

export type DemoIcon = "disc" | "film" | "archive" | "music" | "file";
export type DemoStatus = "downloading" | "completed" | "paused";
export type DemoCategory = "program" | "video" | "audio" | "archive" | "document";

export interface DemoTask {
  id: string;
  name: string;
  size: number;
  icon: DemoIcon;
  category: DemoCategory;
  status: DemoStatus;
  /** 下载中/暂停任务的起始进度(0..1) */
  progress: number;
  maxConn: number;
  /** 每连接平均速度(字节/秒) */
  perConn: number;
}

const MB = 1024 * 1024;
const GB = 1024 * MB;

export const DEMO_TASKS: DemoTask[] = [
  { id: "blender", name: "blender-4.2.1-macos-arm64.dmg", size: 312 * MB, icon: "disc", category: "program", status: "downloading", progress: 0.46, maxConn: 8, perConn: 2.6 * MB },
  { id: "sintel", name: "sintel-4k.mkv", size: 1.94 * GB, icon: "film", category: "video", status: "downloading", progress: 0.63, maxConn: 6, perConn: 1.9 * MB },
  { id: "arch", name: "archlinux-2026.09.01-x86_64.iso", size: 1.21 * GB, icon: "disc", category: "program", status: "completed", progress: 1, maxConn: 1, perConn: 0 },
  { id: "bbb", name: "big_buck_bunny_1080p.mp4", size: 276 * MB, icon: "film", category: "video", status: "completed", progress: 1, maxConn: 1, perConn: 0 },
  { id: "fonts", name: "noto-cjk-fonts-2.004.zip", size: 1.07 * GB, icon: "archive", category: "archive", status: "completed", progress: 1, maxConn: 1, perConn: 0 },
  { id: "talk", name: "rustconf-keynote.m4a", size: 86 * MB, icon: "music", category: "audio", status: "paused", progress: 0.58, maxConn: 4, perConn: 0 },
  { id: "manual", name: "gpui-architecture.pdf", size: 18.4 * MB, icon: "file", category: "document", status: "completed", progress: 1, maxConn: 1, perConn: 0 },
];

/** 演示中「新建下载」加入的任务 */
export const DEMO_NEW: DemoTask = {
  id: "debian",
  name: "debian-12.7.0-amd64-netinst.iso",
  size: 631 * MB,
  icon: "disc",
  category: "program",
  status: "downloading",
  progress: 0,
  maxConn: 16,
  perConn: 7 * MB,
};

export const DEMO_URL = "https://cdimage.debian.org/debian-cd/12.7.0/amd64/iso-cd/debian-12.7.0-amd64-netinst.iso";
export const DEMO_SAVE_DIR = "~/Downloads/Programs";
export const DEMO_FREE = "416.3 GB";
