import { defineMessages } from "../define";

/**
 * 支持类页面文案:macOS Gatekeeper 指南、安全警告(仿冒站)、404。
 * 终端命令在两种语言下保持完全一致,只翻译说明文字。
 */
const CMD = {
  anywhere: "sudo spctl --master-disable",
  quarantine: "sudo xattr -rd com.apple.quarantine /Applications/FluxDown.app",
  clt: "xcode-select --install",
  codesign: "sudo codesign --force --deep --sign - /Applications/FluxDown.app",
} as const;

export const guides = defineMessages({
  en: {
    terminal: { label: "Terminal", copy: "Copy", copied: "Copied", copyAria: "Copy command" },
    macos: {
      meta: {
        title: "Fix macOS 'App is Damaged' Error - FluxDown",
        description:
          "How to open FluxDown on macOS when it says the app is 'damaged' or from an 'unidentified developer': allow apps from Anywhere, remove the quarantine flag, or sign locally.",
      },
      eyebrow: "Guide · macOS",
      title: "Fix the macOS <em>'damaged'</em> error",
      lede: "Because FluxDown is not signed with an Apple certificate, macOS may block it from opening. The steps below will help you get it running.",
      updated: "Last updated: 2025",
      whyTitle: "Why does this happen?",
      whyDesc:
        "macOS Gatekeeper only allows apps from the App Store or identified developers by default. FluxDown is free and open-source and does not yet hold an Apple developer certificate, so macOS shows 'damaged' or 'unidentified developer' warnings. This does not mean the app itself is harmful.",
      methodLabel: (n: number) => `Method ${String(n).padStart(2, "0")}`,
      methods: [
        {
          title: "Allow apps from Anywhere",
          tag: "recommended",
          steps: [
            {
              text: "Open System Settings → Privacy & Security → General and check whether the 'Anywhere' option is already enabled.",
              code: null,
              note: null,
            },
            {
              text: "If the option is missing, open Terminal and run the command below, then press Return and enter your password:",
              code: CMD.anywhere,
              note: null,
            },
            {
              text: "After the command completes, go back to System Settings → Privacy & Security and you will see 'Anywhere' is now enabled.",
              code: null,
              note: null,
            },
          ],
        },
        {
          title: "Remove the quarantine flag",
          tag: "if still blocked",
          steps: [
            {
              text: "If enabling 'Anywhere' is not enough, use the following command to remove the quarantine attribute from the app:",
              code: CMD.quarantine,
              note: "Replace FluxDown.app with your actual app name, or drag the app from Finder into the Terminal window after the command. Make sure there is a space after quarantine.",
            },
          ],
        },
        {
          title: "Local code signing",
          tag: "last resort",
          steps: [
            { text: "First install Command Line Tools:", code: CMD.clt, note: null },
            {
              text: "Click Continue in the installer window and wait for it to finish. Then run the command below to sign the app locally:",
              code: CMD.codesign,
              note: "Replace the app path with the actual path. You can drag the app from Finder → Applications into the Terminal window.",
            },
          ],
        },
      ],
      tipTitle: "Tip",
      tipDesc:
        "Method 1 resolves the issue for ~85% of cases, and Method 2 covers ~90%. If none of the methods work, please let us know in our community.",
      feedback: "Report a problem",
      back: "Back to Downloads",
    },
    security: {
      meta: {
        title: "Security Alert: Fake FluxDown Site - FluxDown",
        description:
          "A fake site (fluxdown.com.cn) is impersonating FluxDown and distributing the SilverFox remote-access trojan. Learn which domains are official and what to do if you downloaded from the fake site.",
      },
      eyebrow: "Security notice",
      title: "Security <em>Alert</em>",
      lede: 'We have found unofficial sites impersonating FluxDown and distributing malicious installers carrying the "SilverFox" remote-access trojan. Do not download or run any files from such sites to avoid loss of funds and privacy.',
      fakeLabel: "Fake site (do NOT visit)",
      fakeSite: "fluxdown.com.cn",
      officialLabel: "Official sites (trust only these domains)",
      primary: "primary",
      adviceTitle: "If you downloaded from the fake site",
      advice: [
        "Delete the installer immediately and do not run it.",
        "If you already ran it, disconnect from the network and run a full scan with reputable antivirus software — SilverFox is a remote-access trojan.",
        "Change passwords for important accounts from a clean device, and check payment/banking accounts for unusual activity.",
      ],
      verifyTitle: "How to verify a download",
      verify: [
        "Check the address bar: the domain must exactly match one of the official sites listed above. Look-alike domains with extra suffixes (such as .com.cn) are not ours.",
        "Release files are published on GitHub under zerx-lab/FluxDown. Any other GitHub account or mirror is unofficial.",
        "Windows installers from official channels are code-signed. Right-click the file → Properties → Digital Signatures to confirm a valid signature is present.",
      ],
      signingLink: "Code signing policy",
      downloadTitle: "Official download channels",
      downloadNote:
        "Only download FluxDown from the official sites above or from GitHub Releases. We never distribute installers through third-party download stations.",
      downloadCta: "Download from official site",
    },
    notFound: {
      title: "Page not found",
      desc: "The page you're looking for doesn't exist or has been moved.",
      readout: "segment 05 · http 404 · not found",
      home: "Back to Home",
      docs: "Documentation",
      download: "Download",
      feedback: "Send Feedback",
    },
  },
  zh: {
    terminal: { label: "终端", copy: "复制", copied: "已复制", copyAria: "复制命令" },
    macos: {
      meta: {
        title: "macOS「已损坏」错误解决方案 - FluxDown",
        description:
          "FluxDown 在 macOS 上提示「已损坏，无法打开」或「来自身份不明的开发者」的解决方法，包含开启任何来源、绕过 Gatekeeper 等三种方式。",
      },
      eyebrow: "指南 · macOS",
      title: "macOS <em>「已损坏」</em>错误解决方案",
      lede: "由于应用未经 Apple 官方签名，macOS 可能会阻止运行。以下方法可帮助你顺利打开 FluxDown。",
      updated: "最后更新：2025 年",
      whyTitle: "为什么会出现这个错误？",
      whyDesc:
        "macOS 的 Gatekeeper 安全机制默认只允许运行来自 App Store 或已认证开发者的应用。FluxDown 是免费软件，暂未购买 Apple 开发者证书进行签名，因此系统会显示「已损坏」或「来自身份不明的开发者」的提示。这不代表软件本身有问题。",
      methodLabel: (n: number) => `方法 ${String(n).padStart(2, "0")}`,
      methods: [
        {
          title: "开启「任何来源」",
          tag: "推荐",
          steps: [
            {
              text: "打开「系统设置」→「隐私与安全性」→「通用」，检查是否已有「任何来源」选项。",
              code: null,
              note: null,
            },
            {
              text: "如果没有该选项，打开「终端」，输入以下命令后按回车，并输入密码确认：",
              code: CMD.anywhere,
              note: null,
            },
            {
              text: "执行完毕后，返回「系统设置」→「隐私与安全性」即可看到「任何来源」已启用。",
              code: null,
              note: null,
            },
          ],
        },
        {
          title: "绕过 Gatekeeper 隔离",
          tag: "仍无法打开时",
          steps: [
            {
              text: "开启「任何来源」后仍无法打开时，使用以下命令移除应用的隔离标记：",
              code: CMD.quarantine,
              note: "将 FluxDown.app 替换为你实际的应用名称，或将应用从「访达」直接拖入终端命令末尾。注意 quarantine 后面有一个空格。",
            },
          ],
        },
        {
          title: "本地签名",
          tag: "最后手段",
          steps: [
            { text: "先安装 Command Line Tools：", code: CMD.clt, note: null },
            {
              text: "弹出安装窗口后点击「继续安装」，等待完成。然后执行以下命令对应用进行本地签名：",
              code: CMD.codesign,
              note: "将「应用路径」替换为应用实际路径，可从「访达」→「应用程序」将应用拖入命令末尾。",
            },
          ],
        },
      ],
      tipTitle: "小提示",
      tipDesc:
        "以上方法中，方法一通常可以解决 85% 以上的问题，方法二可覆盖到约 90%。如果三种方法都无效，欢迎到我们的社区反馈。",
      feedback: "反馈问题",
      back: "返回下载页面",
    },
    security: {
      meta: {
        title: "安全警告：仿冒 FluxDown 网站 - FluxDown",
        description:
          "仿冒网站 fluxdown.com.cn 冒充 FluxDown 传播「银狐」远控木马。了解哪些域名是官方的，以及从仿冒网站下载后应该怎么做。",
      },
      eyebrow: "安全公告",
      title: "安全<em>警告</em>",
      lede: "我们发现有非官方网站冒充 FluxDown，传播携带「银狐」远控木马的恶意安装包。请勿在该网站下载或运行任何文件，谨防资金与隐私损失。",
      fakeLabel: "仿冒网站（请勿访问）",
      fakeSite: "fluxdown.com.cn",
      officialLabel: "官方网址（认准以下域名）",
      primary: "主域名",
      adviceTitle: "如果你从仿冒网站下载过",
      advice: [
        "立即删除安装包，不要运行。",
        "如果已经运行过，请先断网，并使用可靠的杀毒软件全盘查杀——「银狐」是远程控制木马。",
        "在干净的设备上修改重要账号密码，并检查支付、网银账户有无异常。",
      ],
      verifyTitle: "如何核实下载来源",
      verify: [
        "检查地址栏：域名必须与上方列出的官方网址完全一致。带有额外后缀的相似域名（例如 .com.cn）都不是我们的。",
        "发布文件在 GitHub 的 zerx-lab/FluxDown 仓库发布。任何其他 GitHub 账号或镜像都是非官方的。",
        "官方渠道提供的 Windows 安装包带有代码签名。右键文件 →「属性」→「数字签名」，确认存在有效签名。",
      ],
      signingLink: "代码签名政策",
      downloadTitle: "官方下载渠道",
      downloadNote:
        "请只从上方官方网站或 GitHub Releases 下载 FluxDown，我们从不通过第三方下载站分发安装包。",
      downloadCta: "去官网下载",
    },
    notFound: {
      title: "页面未找到",
      desc: "你访问的页面不存在或已被移动。",
      readout: "segment 05 · http 404 · not found",
      home: "返回首页",
      docs: "文档",
      download: "下载",
      feedback: "发送反馈",
    },
  },
});
