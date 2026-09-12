# JustLocation 面板

`ui/` 是独立的 React + TypeScript 控制端，包含自己的依赖、源码和测试。它通过 [integration/CONTRACT.md](../integration/CONTRACT.md) 约定的命令与后台交互，不导入 Rust、C++ 或 Android 的实现代码。

模块目前不打包 Web UI。现有界面已决定重做，视觉依据要先补原版真机截图；规范化阶段先整理模块与代码职责，视觉重做另行推进。

## 独立开发

在 `ui/` 下执行：

```sh
npm ci              # 按锁文件安装依赖
npm run dev         # 本地 Vite 开发服务器
npm test            # Vitest 单元与交互测试
npm run check       # TypeScript 检查
npm run test:e2e    # Playwright 浏览器测试
npm run build       # 检查并生成 ui/dist/
```

也可以在仓库根目录执行 `node build.mjs ui` 或 `node build.mjs test:e2e`，统一入口会设置缓存路径。Windows 上优先使用根入口，避免 PowerShell 的 npm shim 和子进程输出捕获问题。

浏览器没有 KernelSU 环境时，面板会提示从管理器打开。浏览器测试通过 `e2e/` 中的 `window.ksu` 替身模拟平台接口。

首次运行浏览器测试前安装 Chromium：

```sh
PLAYWRIGHT_BROWSERS_PATH=<仓库根>/build/cache/playwright node node_modules/playwright/cli.js install chromium --only-shell
```

## 目录与边界

| 路径 | 职责 |
|---|---|
| `src/` | 面板实现及同目录的单元、交互测试 |
| `src/control.ts` | 后台控制协议封装及现有类型 |
| `e2e/` | 本模块的浏览器测试与平台替身 |
| `reference/` | 原版资源、历史参考与截图，保留原始材料 |
| `build.mjs` | 接入仓库构建入口；实际任务由 package.json 定义 |
| `dist/` | 生成的前端产物，不提交、当前不进入模块 ZIP |

面板只发命令和展示状态，系统接入归 `zygisk/`，模拟状态和持久化归 `backend/`，摇杆服务归 `app/joystick/`。跨模块协议保存在 `integration/`。

`reference/screenshots/` 目前保存的是本面板渲染图，原版真机截图待补；不能把资源 XML 或现有页面当作已经确认的最终视觉规范。
