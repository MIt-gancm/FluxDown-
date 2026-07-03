---
title: Quick Start
description: Create your first download, control tasks, and set up queues and speed limits.
section: getting-started
order: 2
---

This walks through the full flow of creating a download in FluxDown, controlling it while it runs, and organizing tasks with queues and speed limits. See [Installation](/docs/en/getting-started/installation/) if you haven't set up FluxDown yet, and [Interface Overview](/docs/en/getting-started/interface/) for a tour of every panel mentioned here.

## Create Your First Download

Click **New Download** in the top bar (or press <kbd>Ctrl</kbd>/<kbd>Cmd</kbd>+<kbd>F</kbd> to search, then open it from a result). The dialog fields, top to bottom:

| Field | What it does |
|---|---|
| **Download URL** | A multi-line box — paste one URL per line to queue a batch download (magnet and `ed2k://` links work too), or a single URL for one task. FluxDown shows a live count of parsed URLs. Use **Open .torrent file** to pick a local `.torrent`, or **Import TXT file** to load a list of URLs from a text file. |
| **Save Directory** | Where the file lands. Defaults to your global save directory (**Settings → Download**), or your last-used folder if **Remember Last Save Location** is on. |
| **Threads** | Segments to split the download into: **Auto** (FluxDown picks based on file size and CPU count), a fixed preset (4/8/16/32/64), or a custom value from 1–256. Hidden for magnet links and `.torrent` files, since BitTorrent manages its own connections. |
| **Rename (optional)** | Override the detected filename. Only shown for a single URL — batch downloads and torrents always use the detected/embedded name. |
| **Queue** | Assign the task to a named queue instead of the default one. Only appears once you've created at least one queue (see below). |

Click **Advanced Options** to reveal per-task overrides that default to your global settings when left empty:

| Field | What it does |
|---|---|
| **Task Proxy** | A proxy just for this task, e.g. `socks5://127.0.0.1:1080` or `http://host:port`. Leave empty to use the global proxy. Doesn't apply to BitTorrent downloads. |
| **User-Agent** | Pick a preset (Chrome, Firefox, Edge, Safari, or `netdisk` for Baidu Pan direct links) or type a custom string. |
| **Cookie** | Raw `name=value; name2=value2` pairs, for downloads that require you to be logged in. |
| **Hash Verification** | Pick an algorithm (MD5/SHA-1/SHA-256/SHA-512) and paste the expected hash. FluxDown verifies the file after download and flags a mismatch. Leave blank to skip. |
| **Custom Headers** | Extra HTTP headers as key/value rows (use **+ Add header** for more). Use the Cookie field above for cookies rather than a `Cookie` header here. |

Click **Start** (or **Download N files** for a batch) to begin.

<!-- TODO(screenshot): 新建下载对话框,展示 URL 多行输入 + 展开的高级选项 -->

## Controlling Tasks

- **Double-click** a row: pauses an active task, resumes a paused/failed one, or opens the file if it's already complete.
- **Right-click** a row for the full menu: Pause/Resume, Boost Download (see below), Open File (completed tasks only) / Open Folder, Copy URL, Delete Task, and Delete Task & File.
- **Multiple tasks at once**: press <kbd>Ctrl</kbd>/<kbd>Cmd</kbd>+<kbd>A</kbd> or click the manage-mode toggle next to the file name column to check tasks individually, then use the batch action bar to delete the selection (with or without the underlying files). Press <kbd>Esc</kbd> to leave manage mode.
- **Pause/Resume everything**: use the Pause All / Resume All buttons in the top bar, or right-click empty space below the list for Start All / Pause All.

## Queues and Speed Limits

A global speed limit lives in the **status bar** at the bottom of the window — click the limit indicator to open a popover with presets (128 KB/s, 512 KB/s, 1 MB/s, 2 MB/s, 5 MB/s) or type a custom KB/s value. It's off (unlimited) by default.

For finer control, create a **named queue** from the **Queues** section of the sidebar (click the **+** button). Each queue has its own:

- Speed limit in KB/s (0 = unlimited)
- Max concurrent downloads (0 = use the global setting)
- Default save directory
- Default thread count (0 = auto)
- Default User-Agent (empty = inherit the global one)

Assign a task to a queue from the Queue dropdown in the New Download dialog. Right-click (or hover) a queue in the sidebar to edit or delete it — deleting a queue moves its tasks back to the default queue. Click a status or queue entry in the sidebar to filter the task list down to it.

The global **Max Concurrent Downloads** cap (default 5) lives in **Settings → Download** and applies across all queues unless a queue overrides it.

## Boost: Prioritize a Download

Need one file now? Right-click any task that isn't finished yet and choose **Boost Download**. FluxDown pauses every other active task and gives the boosted one full bandwidth; a banner appears at the top of the task list showing how many tasks will automatically resume once it completes, with a **Cancel** link to stop early. The boosted row also gets a small lightning-bolt badge. Right-click it again and choose **Cancel Boost** to release it manually.

## After a Download Completes

By default (**Settings → General → Completion Notifications**), FluxDown shows a system notification with **Open File** / **Open Folder** actions — a floating toast in the bottom-right corner on Windows, or a native notification on macOS/Linux. Notifications for multiple near-simultaneous completions are batched together automatically. You can also just double-click the finished task, or right-click it for Open File / Open Folder at any time.
