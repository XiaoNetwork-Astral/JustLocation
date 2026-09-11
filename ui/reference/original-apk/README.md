# 原版 APK 资源索引（只读证据）

这里的文件全部来自 Fake Location 1.5.2（`com.lerist.fakelocation`）的 APK 解包结果，**只静态读取了 XML/位图资源**，没有反编译业务代码。资源名被混淆成 `a`…`z`、`a0`…`a99`、`a_` 这类短名，且**同名可能属于不同类型**（例如 `nl` 同时是 color、dimen、drawable、id、string、style），所以引用时务必带类型前缀：`@string/nl`、`@drawable/nl`。

## 目录

| 目录 | 内容 |
|---|---|
| `layout/` | 全部布局（291 个，含第三方库资源）。原版自有布局与库资源同为混淆短名，无法靠文件名区分，所以整份保留，按下面的对照表取用。 |
| `menu/` | 抽屉菜单 `c.xml` 及其 `-v22` / `-zh` / `-zh-v22` 变体。 |
| `g.xml` | 设置页 —— 原版设置是 `PreferenceScreen`，这是唯一的设置结构定义。 |
| `values/` | `colors.xml`（配色实值）、`dimens.xml`（间距字号实值）、`strings.xml`（英文原文）、`strings-zh.xml`（中文原文）、`strings-zh-rCN.xml`（无障碍补充）、`public.xml`（名字到 id 的全量映射，用于处理同名不同 id）。 |
| `drawable/` | `drawable-anydpi` 下的矢量图标（viewport 24×24），改色即可复用。 |
| `mipmap-xxxhdpi/` | 原版位图资源，含空态插画 `a0.png` 与主界面 FAB 图标。 |

## 需要重点看的布局（来自 `README.md` 的界面清单）

| 文件 | 是什么 |
|---|---|
| `a_.xml` → `ai.xml` → `b9.xml` | 主界面布局链：抽屉骨架 → CoordinatorLayout + 300dp CollapsingToolbar → `LFragmentContainer` 内容容器 |
| `eu.xml` | 抽屉头（背景图 + 应用图标 + 应用名 + PRO 行 + 「解锁专业版」按钮，minHeight 220dp） |
| `ci.xml` | **位置模拟页**（核心页：目标卡 + 区块头 + 列表卡） |
| `cl.xml` | 路线模拟页 |
| `ch.xml` | WIFI 模拟页 |
| `cf.xml` | 独立模拟页 |
| `cg.xml` / `cm.xml` | 附近基站 / 附近WIFI（列表页） |
| `cj.xml` / `ck.xml` | GPS 位置（百度地图版 / Google 地图版） |
| `c7.xml` | **基站模拟面板：240dp 固定宽的锚定小面板**（不是整页） |
| `c9.xml` | **速度/步频面板：240dp 宽弹出面板** |
| `c8.xml` / `c4.xml` / `c5.xml` / `br.xml` | 循环参数 / 路线命名 / 计步器 / WIFI 编辑弹窗 |
| `h0.xml` | **空状态**：80dp 插画（`a0.png`）+「点击右上角 "+" 按钮进行添加」 |
| `cv.xml` | **列表行**：40×40 绿色圆底 + 图标 + 名称 bold 16dp + 「经纬度:」+ 值 |
| `d8.xml` / `d_.xml` | 1dp 横分隔线 / 1px 竖分隔线（`#f2d4d4d4`） |
| `de.xml` | 摇杆面板（设置页 + 位置切换两页） |
| `a7/a8.xml` `ag/ah.xml` `ak/al.xml` `a6/aj.xml` | 三套地图的选择位置 / 路线规划 / 位置编辑页 |
| `cc.xml` / `cd.xml` / `ce.xml` | 登录与绑定手机号（`LViewPager` 的真实用途，不是主界面） |

## 读这些资源时的两个坑

1. **`attrs.xml` 里的属性名也被混淆，且没有反向映射表**，所以 `app:xxx` 这类库属性的语义只能按上下文推断。唯一被证实的是 `app:vv` = `layout_behavior`（因为它的值是未混淆的类名）。
2. **图标字形无法从资源名判断**（多为 PNG 位图）。位图在 `mipmap-xxxhdpi/`，可以直接看图；`drawable/` 里是矢量。
