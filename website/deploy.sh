#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────
# deploy.sh — 旧站（v1，挂在 /v1/）服务器端自动部署脚本
#
# 拉取最新代码 → 重建镜像 → 滚动重启容器 → 清理悬空镜像
#
# 用法（在服务器 website/ 目录执行，或由 CI 远程调用）:
#   ./deploy.sh
#
# 约定:
#   - 仅当 website/ 子树有变更（或容器未运行）时才重建（无变更时快速退出，幂等可重复跑）
#   - 失败立即中止，不会留下半启动状态
#   - 挂载前缀 SITE_BASE 默认 /v1（与 docker-compose.yml 的默认值一致）；
#     回到根部署时设 SITE_BASE= （空串）
# ─────────────────────────────────────────────────────────────
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

# 仓库根目录（website 的上一级）
REPO_DIR="$(git -C "${SCRIPT_DIR}" rev-parse --show-toplevel)"
SUBDIR="$(git -C "${SCRIPT_DIR}" rev-parse --show-prefix)"
SUBDIR="${SUBDIR%/}"
BRANCH="${DEPLOY_BRANCH:-main}"
export SITE_BASE="${SITE_BASE-/v1}"
# 部署戳放在 .git 内：不进构建上下文、不会被 reset 清掉、不会被误提交。
# 仓库检出与 website-v2/ 共用，HEAD 可能已被对方的部署推进，
# 所以「是否需要重建」按本目录子树哈希判断，而不是比对 HEAD。
STAMP="$(git -C "${REPO_DIR}" rev-parse --absolute-git-dir)/fluxdown-${SUBDIR}.deployed"

# ── Docker 调用自适应 sudo ────────────────────
# 当前用户若不在 docker 组（无法免密调用 docker），自动回退到 sudo。
if docker info >/dev/null 2>&1; then
  DOCKER="docker"
else
  DOCKER="sudo docker"
fi

echo "=========================================="
echo "  FluxDown Website — Deploy"
echo "  仓库   : ${REPO_DIR}"
echo "  分支   : ${BRANCH}"
echo "  前缀   : ${SITE_BASE:-/}"
echo "  Docker : ${DOCKER}"
echo "  时间   : $(date '+%Y-%m-%d %H:%M:%S')"
echo "------------------------------------------"

# ── 1. 拉取最新代码 ──────────────────────────
echo "[1/4] 拉取最新代码..."
git -C "${REPO_DIR}" fetch origin "${BRANCH}"
git -C "${REPO_DIR}" reset --hard "origin/${BRANCH}"
REMOTE_SHA="$(git -C "${REPO_DIR}" rev-parse HEAD)"
TREE="$(git -C "${REPO_DIR}" rev-parse "HEAD:${SUBDIR}")"
echo "      HEAD ${REMOTE_SHA:0:8}，${SUBDIR}/ 子树 ${TREE:0:8}"

# 子树没变、前缀没变且容器已在运行 → 无需任何操作
RUNNING="$(${DOCKER} compose ps -q website 2>/dev/null)"
if [ -n "${RUNNING}" ] && [ -f "${STAMP}" ] && [ "$(cat "${STAMP}")" = "${TREE} ${SITE_BASE}" ]; then
  echo "      ${SUBDIR}/ 无变更且容器在运行，无需部署。"
  exit 0
fi

# ── 2. 重建镜像 ──────────────────────────────
echo "[2/4] 重建 Docker 镜像..."
${DOCKER} compose build website

# ── 3. 滚动重启 ──────────────────────────────
echo "[3/4] 启动/重启容器..."
# 先停掉本 compose project 自己的容器
${DOCKER} compose down --remove-orphans >/dev/null 2>&1 || true
# 兜底：清掉任何残留的同名容器（可能由旧的 docker run / 其他 project 创建，
# 不归当前 compose project 管，compose 无法复用其名字而报冲突）
${DOCKER} rm -f fluxdown-website >/dev/null 2>&1 || true
${DOCKER} compose up -d --wait website
echo "${TREE} ${SITE_BASE}" > "${STAMP}"

# ── 4. 清理悬空镜像 ──────────────────────────
echo "[4/4] 清理悬空镜像..."
${DOCKER} image prune -f >/dev/null 2>&1 || true

# ── IndexNow：只有根部署才提交（子路径部署整站 noindex）──
# 脚本从线上 sitemap 抓 URL,在宿主机直接跑(无需容器内 scripts/);
# 失败自身容错退 0,不影响部署结果。给站点几秒完成启动再提交。
if [ -z "${SITE_BASE}" ]; then
  echo "[+] 提交 IndexNow..."
  sleep 5
  node "${SCRIPT_DIR}/scripts/indexnow-ping.mjs" || echo "      (IndexNow 提交跳过/失败,不影响部署)"
fi

echo "------------------------------------------"
echo "  ✓ 部署完成: ${REMOTE_SHA:0:8}"
echo "=========================================="
