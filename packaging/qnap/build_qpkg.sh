#!/bin/sh
# 用 QDK（qnap-dev/QDK 的 qbuild）构建 QNAP QPKG。仅在 Linux CI 上运行。
#
# 用法：build_qpkg.sh <version> <qdk_arch> <bindir> <out_qpkg_path>
#   bindir: 含 fluxdown-agent 与 fluxdownd 两个二进制的目录
#   qdk_arch: x86_64 | arm_64
#
# 前置：qbuild 已在 PATH（CI 内 clone QDK 后 ./InstallToUbuntu.sh install），
#       imagemagick（convert）用于从 assets/logo/fluxdown_logo.png 生成 QDK 要求的 gif 图标。
set -eu

[ $# -eq 4 ] || { echo "usage: $0 <version> <x86_64|arm_64> <bindir> <out_qpkg>" >&2; exit 2; }
VERSION=$1 ARCH=$2 BINDIR=$3 OUT=$4

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
LOGO="$REPO_ROOT/assets/logo/fluxdown_logo.png"

for b in fluxdown-agent fluxdownd; do
	[ -f "$BINDIR/$b" ] || { echo "binary not found: $BINDIR/$b" >&2; exit 1; }
done
command -v qbuild >/dev/null || { echo "qbuild not in PATH (install QDK first)" >&2; exit 1; }
command -v convert >/dev/null || { echo "imagemagick 'convert' not in PATH" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
pkg="$work/FluxDown"

# ── 组装包目录：模板 + 版本号 + 载荷 ──
mkdir -p "$pkg"
cp "$SCRIPT_DIR/qpkg.cfg" "$pkg/"
cp -r "$SCRIPT_DIR/shared" "$pkg/shared"
chmod 755 "$pkg/shared/fluxdown.sh"
sed -i "s/^QPKG_VER=.*/QPKG_VER=\"$VERSION\"/" "$pkg/qpkg.cfg"

cp "$SCRIPT_DIR/package_routines" "$pkg/"

# 两个二进制（agent + daemon 必须同目录）放入 arch 专属目录（qbuild --build-arch 只打对应架构）；Web UI 已编译期
# 内嵌进二进制，shared/ 只剩启动脚本。
mkdir -p "$pkg/$ARCH"
cp "$BINDIR/fluxdown-agent" "$BINDIR/fluxdownd" "$pkg/$ARCH/"
chmod 755 "$pkg/$ARCH/fluxdown-agent" "$pkg/$ARCH/fluxdownd"

# ── 图标（QDK 要求 gif：64x64 / 80x80 / 64x64 灰度） ──
mkdir -p "$pkg/icons"
convert "$LOGO" -resize 64x64 "$pkg/icons/FluxDown.gif"
convert "$LOGO" -resize 80x80 "$pkg/icons/FluxDown_80.gif"
convert "$LOGO" -resize 64x64 -colorspace Gray "$pkg/icons/FluxDown_gray.gif"

# ── 构建 ──
(cd "$pkg" && qbuild --build-arch "$ARCH")

built=$(find "$pkg/build" -name '*.qpkg' | head -n 1)
[ -n "$built" ] || { echo "qbuild produced no .qpkg" >&2; exit 1; }
mkdir -p "$(dirname "$OUT")"
cp "$built" "$OUT"
echo "built: $OUT"
