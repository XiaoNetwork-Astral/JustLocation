# JustLocation 面板（ui/）

这个目录是**完全独立**的 KernelSU WebUI 控制面板：自带 `package.json`、测试与参考材料，不依赖仓库里的 Rust/C++/Android 代码，也不需要手机或模块就能开发和验证。它与模块之间只有一层约定——`docs/CONTRACT.md` 里的后台协议。

> 现状：界面还在重做中，视觉与版式要对齐 Fake Location 1.5.2（见 `reference/README.md`）。当前实现只完成到"三段式骨架"，细节与其余页面仍是过渡状态。

## 单独开发

```sh
npm install          # 依赖已锁定，CI 可用 npm ci
npm run dev          # http://127.0.0.1:5173 ，浏览器里直接看
npm test             # vitest（jsdom）
npm run check        # tsc --noEmit
npm run test:e2e     # Playwright，见下方环境变量
npm run build        # 产出 dist/，供模块打包
```

浏览器里没有 KernelSU 环境，面板会显示「请从 KernelSU 管理器打开面板」，这是预期行为：`reference/` 与 `e2e/` 里有现成的 `window.ksu` 桩，照抄即可在本地跑通完整交互（`e2e/panel.spec.ts` 开头那份 `addInitScript` 就是模板）。

运行浏览器测试前需要 Chromium：

```sh
PLAYWRIGHT_BROWSERS_PATH=<仓库根>/build/cache/playwright node node_modules/playwright/cli.js install chromium --only-shell
```

Windows 上有两个环境限制要注意：**不要用 PowerShell 管道捕获 node 子进程输出**（会 `spawn EPERM`），改成 `cmd /c "node ... > 日志 2>&1"` 再读日志；npm 的 `.ps1` shim 也不可用，直接 `node node_modules/...`。

## 目录

| 路径 | 说明 |
|---|---|
| `src/` | 面板源码 |
| `src/control.ts` | 与后台的协议封装（`createClient`），以及坐标解析 |
| `src/backend.ts` **（待建）** | 规划中的唯一后台入口：把 `control/cells/wifis/joystick/fileExport` 收成一个模块，界面只依赖它 |
| `e2e/` | Playwright 用例，含 `window.ksu` 桩 |
| `reference/README.md` | **界面的唯一设计真源**：原版结构、尺寸、配色、文案、交互习惯，以及本项目的差异与硬约束 |
| `reference/original-apk/` | 原版 APK 的布局/菜单/颜色/尺寸/位图，以及一份取用索引 |
| `reference/screenshots/` | 截图（目前只有本面板的渲染，原版截图待补，见下） |

## 与模块的对接方式

- 构建产物 `dist/` 会被 `build.mjs` 拷进模块的 `webroot/`，管理器从 KernelSU 的 WebUI 打开它。
- 面板通过 KernelSU 的 `exec` 调用 `/data/adb/modules/justlocation/bin/justlocationd request <base64>` 读写后台；协议细节见 `docs/CONTRACT.md`。
- 面板**不做**任何系统 Hook：它只发命令、显示状态。系统侧的行为由模块的 Rust 后台与桥接 DEX 负责。

## 缺什么（需要人或另一次会话补）

1. **原版的真实截图**。`reference/screenshots/` 目前只有本面板自己的渲染图。原版 APK 只带 ARM 原生库，本机的 x86_64 模拟器装不上，所以只能在一台 ARM64 真机上装一次原版、逐页截图再卸载。没有这些图，间距、层次、折叠行为这类"资源 XML 看不出来"的东西就只能靠推断。
2. **P1 及以后的版式**：240dp 锚定小面板（基站、速频）、抽屉三组折叠、白药丸分段、地图页 FAB 组与绿色路线搜索卡、设置分类长列表、观测三页。清单与优先级见 `reference/README.md`。
3. **旧样式收尾**：路线页与 Wi-Fi 页还在用旧的 `.section-heading`，应换成 `src/SectionHeader.tsx` / `src/RowCard.tsx`。
