import { defineMessages } from "../define";

/**
 * 法务类长文(隐私政策 / 服务条款 / 代码签名政策)。
 *
 * 每篇文档是 `sections` 数组;`body` 与 `items` 允许内联 HTML(链接、强调、代码),
 * 由 `LegalDoc` 以 `set:html` 渲染——内容均为本文件中的静态字面量,不接受外部输入。
 */
const GH = "https://github.com/zerx-lab/FluxDown";
const ext = (url: string, label: string) =>
  `<a href="${url}" rel="noopener noreferrer" target="_blank">${label}</a>`;

export const legal = defineMessages({
  en: {
    index: "On this page",
    updated: "Last updated",
    privacy: {
      meta: {
        title: "Privacy Policy - FluxDown",
        description:
          "FluxDown privacy policy. No telemetry, no tracking, no accounts required. All download data stays 100% local on your device.",
      },
      eyebrow: "Legal · Privacy",
      title: "Privacy <em>Policy</em>",
      updated: "September 2026",
      intro:
        'FluxDown ("we", "our", or "the software") is committed to protecting your privacy. This Privacy Policy explains what information we collect, what we do not collect, and how your data is handled when you use FluxDown desktop application, browser extension, and this website.',
      sections: [
        {
          id: "collect",
          title: "Information We Collect",
          body: "We collect minimal information necessary to provide and improve our services:",
          items: [
            "<strong>Website analytics:</strong> We use Vercel Web Analytics, a privacy-friendly analytics service that collects anonymous, aggregated page view data. No cookies are used, no personal data is collected, and no individual users are tracked.",
            "<strong>Feedback submissions:</strong> When you voluntarily submit feedback through our website, we collect the feedback type, title, description, and optionally your contact information. This data is stored as GitHub Issues in our repository.",
            "<strong>Email subscriptions:</strong> If you subscribe to platform availability notifications, we store your email address in a GitHub Issue. This is used solely to notify you when the requested platform becomes available.",
            "<strong>Mirror speed statistics (optional):</strong> To power the Community Speed Network, FluxDown may report anonymous, aggregated mirror speed metrics (region-level mirror + measured throughput). These contain no URLs, file names, IP addresses, or identifiers of any kind, and can be disabled in settings at any time.",
          ],
        },
        {
          id: "not-collect",
          title: "Information We Do NOT Collect",
          body: "FluxDown is designed with a local-first architecture. The desktop application:",
          items: [
            "Does NOT collect, transmit, or store any of your download URLs, file names, or download history on any remote server. All download data is stored locally in a SQLite database on your device.",
            "Does NOT include any crash reporting or usage analytics. The only optional network report is the anonymous mirror speed metric described above — and it can be turned off.",
            "Does NOT require an account, login, or registration of any kind.",
            "Does NOT phone home for anything else. Aside from downloads you initiate and the optional speed metrics, the application operates entirely offline.",
          ],
        },
        {
          id: "extension",
          title: "Browser Extension",
          body: "The FluxDown browser extension communicates exclusively with the FluxDown desktop application via a local HTTP endpoint (<code>localhost:19527</code>). Specifically:",
          items: [
            "All data transfer occurs locally on your machine between the browser extension and the desktop application. No data is sent to any external server.",
            "The extension stores your preferences (auto-intercept toggle, file type filters, domain rules) in your browser's local storage (<code>chrome.storage.sync/local</code>). This data syncs only through your browser's built-in sync mechanism, if enabled.",
            "The extension does not access, read, or modify any web page content beyond intercepting download requests.",
          ],
        },
        {
          id: "analytics",
          title: "Website Analytics",
          body: "This website uses Vercel Web Analytics, which is a privacy-focused analytics service. It does not use cookies, does not collect personal information, does not track individual users across sessions, and is compliant with GDPR, CCPA, and other privacy regulations without requiring a cookie consent banner.",
          items: [],
        },
        {
          id: "storage",
          title: "Data Storage & Security",
          body: "All download data (task records, file paths, progress) is stored locally on your device in a SQLite database managed by the FluxDown application. We have no access to this data. Website feedback and subscription data is stored in GitHub Issues within our repository, subject to GitHub's privacy policy.",
          items: [],
        },
        {
          id: "third-party",
          title: "Third-Party Services",
          body: "Our website interacts with the following third-party services:",
          items: [
            "<strong>GitHub API:</strong> Used server-side to fetch release information and process feedback submissions. Your IP address is not forwarded to GitHub.",
            "<strong>Fonts:</strong> All web fonts are self-hosted on this website. No font requests are made to Google Fonts or any other third-party font service.",
          ],
        },
        {
          id: "children",
          title: "Children's Privacy",
          body: "FluxDown does not knowingly collect any personal information from children under the age of 13. Since the desktop application collects no personal data at all, and website data collection is limited to voluntary feedback submissions, we believe our service is inherently safe for users of all ages.",
          items: [],
        },
        {
          id: "changes",
          title: "Changes to This Policy",
          body: "We may update this Privacy Policy from time to time. Changes will be posted on this page with an updated revision date. We encourage you to review this page periodically.",
          items: [],
        },
        {
          id: "contact",
          title: "Contact Us",
          body: 'If you have any questions about this Privacy Policy, please reach out to us through the <a href="{feedback}">Feedback page</a> on our website.',
          items: [],
        },
      ],
    },
    terms: {
      meta: {
        title: "Terms of Service - FluxDown",
        description:
          "FluxDown terms of service: license, acceptable use, warranties and liability for the FluxDown desktop app, browser extension and website.",
      },
      eyebrow: "Legal · Terms",
      title: "Terms of <em>Service</em>",
      updated: "February 2026",
      intro:
        'Please read these Terms of Service ("Terms") carefully before using FluxDown software, browser extension, and website (collectively, the "Service"). By using the Service, you agree to be bound by these Terms.',
      sections: [
        {
          id: "acceptance",
          title: "Acceptance of Terms",
          body: "By downloading, installing, or using FluxDown, you agree to these Terms. If you do not agree, please do not use the Service. We reserve the right to update these Terms at any time, and continued use constitutes acceptance of any changes.",
          items: [],
        },
        {
          id: "license",
          title: "License",
          body: "FluxDown is provided as free software. Subject to these Terms, we grant you a non-exclusive, non-transferable, revocable license to use the software for personal or commercial purposes. You may:",
          items: [
            "Download, install, and use FluxDown on any number of devices you own or control.",
            "Use the browser extension alongside the desktop application.",
            "Share the official download link with others.",
          ],
        },
        {
          id: "acceptable-use",
          title: "Acceptable Use",
          body: "You agree to use FluxDown only for lawful purposes. You must NOT use the Service to:",
          items: [
            "Download content that infringes upon the intellectual property rights of others, including copyrighted material without authorization.",
            "Violate any applicable local, national, or international laws or regulations.",
            "Attempt to reverse-engineer, decompile, or disassemble the software, except as permitted by applicable law.",
            "Distribute modified versions of the software under the FluxDown name without explicit permission.",
          ],
        },
        {
          id: "ip",
          title: "Intellectual Property",
          body: "The FluxDown name, logo, and associated branding are the intellectual property of the FluxDown project. The software's source code is subject to its respective license terms. All content on this website, including text, graphics, and design, is owned by FluxDown unless otherwise noted.",
          items: [],
        },
        {
          id: "warranties",
          title: "Disclaimer of Warranties",
          body: 'FluxDown is provided "AS IS" and "AS AVAILABLE" without warranties of any kind, either express or implied, including but not limited to implied warranties of merchantability, fitness for a particular purpose, and non-infringement. We do not warrant that the Service will be uninterrupted, error-free, or free of harmful components.',
          items: [],
        },
        {
          id: "liability",
          title: "Limitation of Liability",
          body: "To the maximum extent permitted by applicable law, FluxDown and its contributors shall not be liable for any indirect, incidental, special, consequential, or punitive damages, or any loss of profits or revenues, whether incurred directly or indirectly, or any loss of data, use, goodwill, or other intangible losses resulting from your use of the Service.",
          items: [],
        },
        {
          id: "feedback",
          title: "User Content & Feedback",
          body: "When you submit feedback, bug reports, or feature requests through our website, you grant us a non-exclusive, worldwide, royalty-free license to use, reproduce, and display such content for the purpose of improving FluxDown. We will not share your contact information with third parties.",
          items: [],
        },
        {
          id: "termination",
          title: "Termination",
          body: "You may stop using FluxDown at any time by uninstalling the software and removing the browser extension. We reserve the right to modify or discontinue the Service at any time without prior notice. Upon termination, all provisions of these Terms that by their nature should survive will remain in effect.",
          items: [],
        },
        {
          id: "changes",
          title: "Changes to These Terms",
          body: "We reserve the right to modify these Terms at any time. Updated Terms will be posted on this page with a revised date. Your continued use of the Service after any changes indicates your acceptance of the new Terms.",
          items: [],
        },
        {
          id: "contact",
          title: "Contact Us",
          body: 'If you have any questions about these Terms of Service, please reach out to us through the <a href="{feedback}">Feedback page</a> on our website.',
          items: [],
        },
      ],
    },
    signing: {
      meta: {
        title: "Code Signing Policy - FluxDown",
        description:
          "FluxDown code signing policy. Free code signing provided by SignPath.io, certificate by SignPath Foundation.",
      },
      eyebrow: "Trust · Code signing",
      title: "Code Signing <em>Policy</em>",
      updated: "July 10, 2026",
      intro: `This project signs and distributes release artifacts. Free code signing provided by ${ext("https://about.signpath.io", "SignPath.io")}, certificate by ${ext("https://signpath.org", "SignPath Foundation")}.`,
      pipeline: {
        caption: "signing pipeline · windows",
        steps: [
          { k: "source", v: "zerx-lab/FluxDown" },
          { k: "build", v: "GitHub Actions CI" },
          { k: "approve", v: "Maintainer, manual" },
          { k: "sign", v: "SignPath · HSM key" },
          { k: "publish", v: "GitHub Releases" },
        ],
      },
      sections: [
        {
          id: "what",
          title: "What is signed",
          body: "",
          items: [
            `Windows installer packages and executables (<code>.exe</code>, <code>.msi</code>) published on ${ext(`${GH}/releases`, "GitHub Releases")}.`,
          ],
        },
        {
          id: "process",
          title: "Build and signing process",
          body: "",
          items: [
            `All artifacts are built from the public repository ${ext(GH, "zerx-lab/FluxDown")} using GitHub Actions CI.`,
            "Only CI-built artifacts are submitted to SignPath for signing.",
            "The private key is held by SignPath on HSM. This project does not store or have access to the private key.",
            "Every signing request requires explicit manual approval by the maintainer.",
          ],
        },
        {
          id: "roles",
          title: "Team roles",
          body: "",
          items: [
            `<strong>Authors</strong> (commit access, can modify the repository without additional reviews): ${ext("https://github.com/zerx-lab", "zerx-lab")}`,
            `<strong>Reviewers</strong> (all external pull requests are reviewed before merge): ${ext("https://github.com/zerx-lab", "zerx-lab")}`,
            `<strong>Approvers</strong> (each signing request requires explicit approval): ${ext("https://github.com/zerx-lab", "zerx-lab")}`,
          ],
        },
        {
          id: "platforms",
          title: "Other platforms",
          body: "",
          items: [
            'macOS: artifacts are currently unsigned; users should obtain artifacts only from the official GitHub Releases page. See <a href="{gatekeeper}">macOS Gatekeeper instructions</a>.',
            "Linux: artifacts (AppImage / deb / Arch / portable) are currently not cryptographically signed; obtain them only from the official GitHub Releases page.",
          ],
        },
        {
          id: "privacy",
          title: "Privacy policy",
          body: 'FluxDown does not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it (e.g. checking for updates on GitHub Releases). See the full <a href="{privacy}">Privacy Policy</a>.',
          items: [],
        },
      ],
    },
  },
  zh: {
    index: "本页目录",
    updated: "最后更新",
    privacy: {
      meta: {
        title: "隐私政策 - FluxDown",
        description: "FluxDown 隐私政策。无遥测、无追踪、无需账户，所有下载数据 100% 保存在你的设备本地。",
      },
      eyebrow: "法律 · 隐私",
      title: "隐私<em>政策</em>",
      updated: "2026 年 9 月",
      intro:
        "FluxDown（以下简称「我们」或「本软件」）致力于保护您的隐私。本隐私政策说明了当您使用 FluxDown 桌面应用、浏览器扩展和本网站时，我们收集哪些信息、不收集哪些信息以及如何处理您的数据。",
      sections: [
        {
          id: "collect",
          title: "我们收集的信息",
          body: "我们仅收集提供和改进服务所必需的最少信息：",
          items: [
            "<strong>网站分析：</strong>我们使用 Vercel Web Analytics，这是一项注重隐私的分析服务，仅收集匿名的、聚合的页面浏览数据。不使用 Cookie，不收集个人数据，不追踪个人用户。",
            "<strong>反馈提交：</strong>当您通过网站自愿提交反馈时，我们收集反馈类型、标题、描述以及可选的联系方式。这些数据以 GitHub Issue 的形式存储在我们的代码仓库中。",
            "<strong>邮件订阅：</strong>如果您订阅了平台可用性通知，我们会将您的邮箱地址存储在 GitHub Issue 中，仅用于在所请求的平台可用时通知您。",
            "<strong>镜像测速统计（可选）：</strong>为支持社区加速网络，FluxDown 可能上报匿名的聚合镜像测速指标（地区级镜像 + 实测速度）。其中不包含任何链接、文件名、IP 地址或任何形式的身份标识，且可随时在设置中关闭。",
          ],
        },
        {
          id: "not-collect",
          title: "我们不收集的信息",
          body: "FluxDown 采用本地优先架构设计。桌面应用程序：",
          items: [
            "不会收集、传输或在任何远程服务器上存储您的下载链接、文件名或下载历史。所有下载数据均存储在您设备上的本地 SQLite 数据库中。",
            "不包含任何崩溃报告或使用行为分析。唯一可选的网络上报是上文所述的匿名镜像测速指标，且可以随时关闭。",
            "不需要任何形式的账户、登录或注册。",
            "不进行其他任何「回连」。除您主动发起的下载和可选的测速指标外，应用完全离线运行。",
          ],
        },
        {
          id: "extension",
          title: "浏览器扩展",
          body: "FluxDown 浏览器扩展仅通过本地 HTTP 端点（<code>localhost:19527</code>）与 FluxDown 桌面应用通信。具体而言：",
          items: [
            "所有数据传输都在您的设备本地进行，发生在浏览器扩展和桌面应用之间。不会向任何外部服务器发送数据。",
            "扩展将您的偏好设置（自动拦截开关、文件类型过滤器、域名规则）存储在浏览器的本地存储中（<code>chrome.storage.sync/local</code>）。这些数据仅通过浏览器自带的同步机制进行同步（如已启用）。",
            "扩展不会访问、读取或修改任何网页内容，仅拦截下载请求。",
          ],
        },
        {
          id: "analytics",
          title: "网站分析",
          body: "本网站使用 Vercel Web Analytics，这是一项注重隐私的分析服务。它不使用 Cookie、不收集个人信息、不跨会话追踪用户，符合 GDPR、CCPA 等隐私法规，无需显示 Cookie 同意横幅。",
          items: [],
        },
        {
          id: "storage",
          title: "数据存储与安全",
          body: "所有下载数据（任务记录、文件路径、进度）均存储在您设备上由 FluxDown 应用管理的本地 SQLite 数据库中。我们无法访问这些数据。网站反馈和订阅数据存储在我们 GitHub 仓库的 Issue 中，受 GitHub 隐私政策约束。",
          items: [],
        },
        {
          id: "third-party",
          title: "第三方服务",
          body: "本网站与以下第三方服务交互：",
          items: [
            "<strong>GitHub API：</strong>在服务端用于获取发布信息和处理反馈提交。您的 IP 地址不会转发至 GitHub。",
            "<strong>字体：</strong>本网站所有网页字体均为自托管，不会向 Google Fonts 或任何第三方字体服务发起请求。",
          ],
        },
        {
          id: "children",
          title: "儿童隐私",
          body: "FluxDown 不会故意收集 13 岁以下儿童的任何个人信息。由于桌面应用完全不收集个人数据，且网站数据收集仅限于自愿提交的反馈，我们认为本服务对所有年龄段的用户都是安全的。",
          items: [],
        },
        {
          id: "changes",
          title: "政策变更",
          body: "我们可能会不时更新本隐私政策。变更将发布在本页面，并更新修订日期。建议您定期查看本页面。",
          items: [],
        },
        {
          id: "contact",
          title: "联系我们",
          body: '如果您对本隐私政策有任何疑问，请通过我们网站的<a href="{feedback}">反馈页面</a>与我们联系。',
          items: [],
        },
      ],
    },
    terms: {
      meta: {
        title: "服务条款 - FluxDown",
        description: "FluxDown 服务条款：FluxDown 桌面应用、浏览器扩展与网站的许可、合理使用、担保与责任限制。",
      },
      eyebrow: "法律 · 条款",
      title: "服务<em>条款</em>",
      updated: "2026 年 2 月",
      intro:
        "请在使用 FluxDown 软件、浏览器扩展和网站（统称「服务」）前仔细阅读本服务条款（以下简称「条款」）。使用本服务即表示您同意受本条款约束。",
      sections: [
        {
          id: "acceptance",
          title: "条款接受",
          body: "通过下载、安装或使用 FluxDown，您同意本条款。如果您不同意，请不要使用本服务。我们保留随时更新本条款的权利，继续使用即表示接受任何更改。",
          items: [],
        },
        {
          id: "license",
          title: "许可",
          body: "FluxDown 作为免费软件提供。在遵守本条款的前提下，我们授予您一项非排他性、不可转让、可撤销的许可，允许您将本软件用于个人或商业目的。您可以：",
          items: [
            "在您拥有或控制的任意数量的设备上下载、安装和使用 FluxDown。",
            "将浏览器扩展与桌面应用配合使用。",
            "与他人分享官方下载链接。",
          ],
        },
        {
          id: "acceptable-use",
          title: "合理使用",
          body: "您同意仅将 FluxDown 用于合法目的。您不得使用本服务：",
          items: [
            "下载侵犯他人知识产权的内容，包括未经授权的受版权保护的材料。",
            "违反任何适用的地方、国家或国际法律法规。",
            "尝试对软件进行逆向工程、反编译或反汇编，除非适用法律允许。",
            "未经明确许可，以 FluxDown 名义分发修改版本的软件。",
          ],
        },
        {
          id: "ip",
          title: "知识产权",
          body: "FluxDown 名称、标志及相关品牌形象是 FluxDown 项目的知识产权。软件源代码受其各自的许可条款约束。本网站上的所有内容，包括文字、图形和设计，除另有说明外，均归 FluxDown 所有。",
          items: [],
        },
        {
          id: "warranties",
          title: "免责声明",
          body: "FluxDown 按「原样」和「可用」的基础提供，不提供任何明示或暗示的担保，包括但不限于对适销性、特定用途适用性和不侵权的暗示担保。我们不保证服务不会中断、无错误或不含有害成分。",
          items: [],
        },
        {
          id: "liability",
          title: "责任限制",
          body: "在适用法律允许的最大范围内，FluxDown 及其贡献者不对任何间接的、附带的、特殊的、后果性的或惩罚性的损害赔偿负责，也不对因您使用本服务而直接或间接产生的任何利润或收入损失、数据丢失、使用损失、商誉损失或其他无形损失负责。",
          items: [],
        },
        {
          id: "feedback",
          title: "用户内容与反馈",
          body: "当您通过网站提交反馈、错误报告或功能请求时，您授予我们非排他性的、全球范围内的、免版税的许可，允许我们使用、复制和展示该内容，以改进 FluxDown。我们不会与第三方分享您的联系信息。",
          items: [],
        },
        {
          id: "termination",
          title: "终止",
          body: "您可以随时通过卸载软件和移除浏览器扩展来停止使用 FluxDown。我们保留随时修改或终止服务的权利，恕不另行通知。终止后，本条款中因其性质应当继续有效的所有条款将继续有效。",
          items: [],
        },
        {
          id: "changes",
          title: "条款变更",
          body: "我们保留随时修改本条款的权利。更新后的条款将发布在本页面，并注明修订日期。在任何更改之后继续使用本服务即表示您接受新条款。",
          items: [],
        },
        {
          id: "contact",
          title: "联系我们",
          body: '如果您对本服务条款有任何疑问，请通过我们网站的<a href="{feedback}">反馈页面</a>与我们联系。',
          items: [],
        },
      ],
    },
    signing: {
      meta: {
        title: "代码签名政策 - FluxDown",
        description: "FluxDown 代码签名政策。免费代码签名由 SignPath.io 提供，证书由 SignPath Foundation 颁发。",
      },
      eyebrow: "信任 · 代码签名",
      title: "代码签名<em>政策</em>",
      updated: "2026 年 7 月 10 日",
      intro: `本项目对发布产物进行签名并分发。免费代码签名由 ${ext("https://about.signpath.io", "SignPath.io")} 提供，证书由 ${ext("https://signpath.org", "SignPath Foundation")} 颁发。`,
      pipeline: {
        caption: "signing pipeline · windows",
        steps: [
          { k: "source", v: "zerx-lab/FluxDown" },
          { k: "build", v: "GitHub Actions CI" },
          { k: "approve", v: "维护者人工审批" },
          { k: "sign", v: "SignPath · HSM 私钥" },
          { k: "publish", v: "GitHub Releases" },
        ],
      },
      sections: [
        {
          id: "what",
          title: "签名范围",
          body: "",
          items: [
            `发布在 ${ext(`${GH}/releases`, "GitHub Releases")} 上的 Windows 安装包与可执行文件（<code>.exe</code>、<code>.msi</code>）。`,
          ],
        },
        {
          id: "process",
          title: "构建与签名流程",
          body: "",
          items: [
            `所有产物均由公开仓库 ${ext(GH, "zerx-lab/FluxDown")} 通过 GitHub Actions CI 构建。`,
            "只有 CI 构建的产物才会提交给 SignPath 签名。",
            "私钥由 SignPath 保存在 HSM 中。本项目不存储、也无法访问该私钥。",
            "每一次签名请求都需要维护者明确的人工审批。",
          ],
        },
        {
          id: "roles",
          title: "团队角色",
          body: "",
          items: [
            `<strong>作者</strong>（拥有提交权限，无需额外审查即可修改仓库）：${ext("https://github.com/zerx-lab", "zerx-lab")}`,
            `<strong>审查者</strong>（所有外部 Pull Request 合并前均经过审查）：${ext("https://github.com/zerx-lab", "zerx-lab")}`,
            `<strong>审批者</strong>（每一次签名请求都需明确审批）：${ext("https://github.com/zerx-lab", "zerx-lab")}`,
          ],
        },
        {
          id: "platforms",
          title: "其他平台",
          body: "",
          items: [
            'macOS：产物目前未签名，请仅从官方 GitHub Releases 页面获取。参见 <a href="{gatekeeper}">macOS Gatekeeper 说明</a>。',
            "Linux：产物（AppImage / deb / Arch / 便携版）目前未做加密签名，请仅从官方 GitHub Releases 页面获取。",
          ],
        },
        {
          id: "privacy",
          title: "隐私政策",
          body: 'FluxDown 不会向其他联网系统传输任何信息，除非用户或安装、运行它的人明确要求（例如在 GitHub Releases 上检查更新）。完整内容请参阅<a href="{privacy}">隐私政策</a>。',
          items: [],
        },
      ],
    },
  },
});
