#!/bin/sh
# 群晖 DSM 7 安装 / 升级向导（同一脚本两用，build_spk.sh 复制为两份）。
# 只问一个问题：下载落到哪个共享文件夹。键 wizard_share_name 被 conf/resource
# 的 data-share worker 引用（不存在则创建、每次启动授权 rw 给 sc-fluxdown），
# postinst/postupgrade 再把它落到 var/share_name 供启动脚本解析路径。
# 升级时默认值取上次保存的名字，避免升级向导把授权换到别的文件夹。
set -eu

DEFAULT="FluxDown"
saved="${SYNOPKG_PKGVAR:-/var/packages/FluxDown/var}/share_name"
if [ -r "$saved" ]; then
	prev=$(tr -d '\r\n' < "$saved")
	[ -n "$prev" ] && DEFAULT="$prev"
fi

# 共享文件夹名校验与 DSM 同款：不含路径分隔符（不支持子目录），1–32 字符。
cat > "${SYNOPKG_TEMP_LOGFILE}" <<EOF
[{
	"step_title": "FluxDown Server",
	"invalid_next_disabled_v2": true,
	"items": [{
		"type": "textfield",
		"desc": "Shared folder for downloads. It is created if missing, and the package user sc-fluxdown is granted read/write on every start. To use other shared folders later, grant sc-fluxdown in Control Panel > Shared Folder > Edit > Permissions > System internal user.",
		"subitems": [{
			"key": "wizard_share_name",
			"desc": "Shared folder",
			"defaultValue": "${DEFAULT}",
			"validator": {
				"allowBlank": false,
				"regex": {
					"expr": "/^[\\\\w.][\\\\w. -]{0,30}[\\\\w.-]\$|^[\\\\w]\$/",
					"errorText": "Shared folder name only; subdirectories are not supported."
				}
			}
		}]
	}]
}]
EOF
