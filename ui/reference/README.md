# 界面复刻参考与当前实现

本文件是 JustLocation 面板的界面真源，由五份调研文档合并而成（2026-09-11 起按用户要求参考 Fake Location 1.5.2 的界面组织，09-11 完成两轮重做）：原版 APK 资源清单、Miuix 设计系统参考、React 重写规格、改版前的 React 结构说明书，以及原先放在 `docs/ui-reference.md` 的参考来源与当前实现说明。合并前的数据没有删减，只去掉了互相重复的部分。

- **原版布局**取自 `D:\project\JustLocation\build\fl-apk`（apktool 解包，包名 `com.lerist.fakelocation`，APK 内应用名 `Fake Location`），**只静态读了 XML 资源**（layout / menu / xml / values\*），没反编译 DEX/smali。任何"由代码决定"的绑定关系都标为**未确定**。
- **Miuix 外观**取自本机参考仓库 KernelSU 管理器（`.deepseek/references/kernelsu`，Kotlin / Compose），只读调研，结论都带依据。
- **老项目**指同一工具的前一版 Kotlin + Jetpack Compose 实现（`.deepseek/backup/app/...`，24 个 .kt 文件）；它**不是**原版的复刻，只借用了三段式主页版式。
- **现状**指本仓库当前要重写的 React 面板（`webui/src/`）。

结论标注口径：**资源级 / 代码级确定**、**推断**（有上下文依据但无法证实）、**未确定**（找不到依据）。凡标"推断"的，实现时按视觉结果对齐即可，不要声称"资源里就是这么写的"。

---

# 一、原版 APK 的界面与版式

## 1.1 反查方法（含"同名不同 id"的处理）

| 项目 | 实际情况 | 处理方式 |
|---|---|---|
| 资源名 | 全部混淆为 `a`…`z`、`a0`…`a99`、`a_` 等短名（AndResGuard 风格） | 直接用混淆名，保证可回溯 |
| `res/values/public.xml` | 478 KB 全量 `<public type name id>` 映射 | 核对"名字 → 类型 → 数值 id"，发现**跨类型同名**问题 |
| **同名不同 id 的风险** | 严重。例：`nl` 同时存在于 attr / color / dimen / drawable / id / string / style 七个类型；`@string/nl`=「WIFI模拟」，`@drawable/nl`=主界面悬浮按钮图标，完全无关 | **本文所有引用一律带类型前缀**（`@string/nl`、`@drawable/nl`、`@id/wr`），不写裸名 |
| 字符串来源 | `res/values/strings.xml` 是**英文**（1449 条）；中文在 `res/values-zh/`（1061 条，简体）与 `res/values-zh-rCN/`（157 条，补无障碍文案） | 中文原文标注取自哪一份 |
| 属性名 | `attrs.xml` 里属性名也被混淆，**没有反向映射表** | 库属性只能**按上下文推断**，一律写"（推断）" |
| 框架属性 | `android:` 前缀保留原义 | 用于取 `statusBarColor`、`text`、`hint` 等确定信息 |

一条有用的证据：`b9.xml` 里 `app:vv="@string/fz"`，而该字符串的值是 `com.google.android.material.appbar.AppBarLayout$ScrollingViewBehavior`；`ai.xml` 里 `app:vv` 的值是 `com.lerist.fakelocation.behavior.FloatingActionButtonSrollingBehavior`。**属性名混淆了但值没混淆，因此可确定 `app:vv` = `layout_behavior`。**

## 1.2 主界面布局链（证据链完整）

```
res/layout/a_.xml            ← 抽屉骨架（DrawerLayout #m_，background=@color/j3=#fafafa）
├─ include @layout/ai        ← 主内容
└─ NavigationView #xa  (layout_gravity=start, background=@color/j3)
     ├─ app:su = @layout/eu    ← 抽屉头（推断 su = headerLayout）
     └─ app:a0d = @menu/c      ← 抽屉菜单（推断 a0d = menu）

res/layout/ai.xml            ← CoordinatorLayout, fitsSystemWindows
├─ AppBarLayout #ex (theme=@style/m, height=@dimen/bj=300dp)
│   └─ CollapsingToolbarLayout #a3w (fitsSystemWindows, app:pt="100.0dp")
│        ├─ ImageView  右上角水印 src=@mipmap/a, marginTop="-60dp", alpha=0.2
│        └─ Toolbar #a3v（标题硬编码 "Fake Location"）
├─ TextView  底部居中：免责声明 @string/bm
├─ include @layout/b9       ← 主内容容器
└─ FloatingActionButton #p0
     app:vv = FloatingActionButtonSrollingBehavior（随滚动隐现）
     app:vt = @id/iz（推断 layout_anchor → 锚在内容容器上）
     app:vu = "mm|a3x"（推断 layout_anchorGravity）
     app:bm = @color/ix（推断 backgroundTint = #008577）
     app:a73 = @drawable/nl（推断 srcCompat），android:tint = @color/a1e（白）
     app:ov = @dimen/fa(16dp)

res/layout/b9.xml            ← 主内容容器（**不是 LViewPager**）
└─ com.lerist.lib.factory.widget.LFragmentContainer #iz
     app:cf="80.0dp"（含义未确定）
     app:vv = AppBarLayout$ScrollingViewBehavior（= layout_behavior，已证实）
```

> 两处需要纠正的常见误判：① 主界面**没有** `LViewPager`，主内容是 `LFragmentContainer`（Fragment 容器）；② `cc/cd/ce.xml` 的 `LViewPager` 是**登录流程的步骤向导**，不是主功能页（见 §1.6）。

## 1.3 抽屉头与抽屉菜单

**抽屉头 `res/layout/eu.xml`**（整页只有 header，主题 `@style/s1` → `cs` → `i3` → `i2`，`windowBackground=@color/a4=@color/sv=#ff303030`，深色底给 centerCrop 图压暗）：

```
FrameLayout
├─ ImageView  背景图 src=@drawable/g8, scaleType=centerCrop, match_parent
└─ LinearLayout(vertical, gravity=bottom, minHeight=@dimen/wf=220dp)
   ├─ ImageView #sd   应用图标 @mipmap/ic_launcher（paddingTop 8dp, marginLeft 16dp）
   ├─ TextView #wi    应用名「Fake Location」，20dp bold，白
   ├─ LinearLayout(horizontal)
   │   ├─ TextView #wh  "PRO:  ---/--"（默认 gone，运行时填专业版有效期）
   │   └─ carbon.TextView #wg  「解锁专业版」（**写死在字体资源里**，非 @string）
   │        绿字 @color/ix + 白底，圆角 2dp（推断），clickable
   └─ FrameLayout
       ├─ TextView（2 行，空文本；运行时填账号信息）
       └─ include @layout/d7   ← 抽屉底部广告条（默认 GONE）
```

摘要：**背景图 + 应用图标 + 应用名 + PRO 到期行（默认隐藏）+ 白底绿字「解锁专业版」按钮（圆角 2dp）+ 底部广告位（默认隐藏）**，最小高度 220dp。

**抽屉菜单 `res/menu/c.xml`** = 1 个 `checkableBehavior="single"` 分组（4 项主功能）+ 3 个折叠子菜单。`menu-v22/c.xml` 是同一份，只多了 `android:iconTint="@color/lj"(#222222)`。

| 分组 | id | 图标资源 | 中文标题 |
|---|---|---|---|
| （无分组，单选） | `@id/wr` | `@mipmap/u` | **位置模拟** |
| 同上 | `@id/x4` | `@drawable/o1` | **路线模拟** |
| 同上 | `@id/xb` | `@drawable/o2` | **WIFI模拟** |
| 同上 | `@id/wn` | `@drawable/nx` | **独立模拟** |
| **View**（子菜单 `@id/wx`） | `@id/x9` | `@mipmap/a7` | GPS位置 |
| 同上 | `@id/x8` | `@mipmap/a1` | 附近基站 |
| 同上 | `@id/x_` | `@mipmap/a8` | 附近WIFI |
| **Settings**（`@id/ww`） | `@id/wy` | `@drawable/o3` | 运行模式 |
| 同上 | `@id/x5` | `@drawable/o4` | 设置 |
| **More**（`@id/wv`） | `@id/x1` | `@drawable/o9` | 评分 |
| 同上 | `@id/x6` | `@drawable/o_` | 分享 |
| 同上 | `@id/wt` | `@drawable/ny` | 反馈 |
| 同上 | `@id/x3` | `@drawable/e5` | 举报 |
| 同上 | `@id/ws` | `@drawable/o7` | 常见问题 |
| 同上 | `@id/x7` | `@mipmap/z` | 使用条款 |
| 同上 | `@id/x0` | `@drawable/e6` | 防范措施 |
| 同上 | `@id/wp` | `@drawable/o5` | 交流讨论 |
| 同上（默认隐藏） | `@id/wm` | `@drawable/o5` | 关于（`android:visible="false"`） |
| 同上 | `@id/wz` | `@drawable/o8` | 更多应用 |
| 同上 | `@id/x2` | `@drawable/o0` | 开发者应用推荐 |
| 同上 | `@id/wu` | `@mipmap/x` | 退出登录 |

- **三个分组标题写死在 XML 里、中英文资源都一样**：`menu/c.xml`、`menu-zh/c.xml`、`menu-v22/c.xml`、`menu-zh-v22/c.xml` 里都是字面量英文 `View` / `Settings` / `More`（不是 `@string`）。运行时是否被代码替换成中文**未确定**。
- **图标字形无法确认**：抽屉图标少数是 PNG 位图，多数同时存在 `drawable-anydpi/*.xml`（矢量，viewport 24×24，单 path）与位图版本；这里只能给资源名，不含字形结论。

## 1.4 主界面分页（容器里的 7 个功能页）

`LFragmentContainer #iz` 里装的页面，由抽屉 4 个主项 + View 组 3 个子项一一对应（标题与页面内文案严格吻合）：

| # | 抽屉项 | 页面布局 | 匹配依据 |
|---|---|---|---|
| 1 | 位置模拟 | `res/layout/ci.xml` | 页内「目标位置」「历史位置」「摇杆」↔ `@string/n5/pw/cf`「位置模拟」 |
| 2 | 路线模拟 | `res/layout/cl.xml` | 页内「目标路线」「历史路线」「起点/终点」 |
| 3 | WIFI模拟 | `res/layout/ch.xml` | 页内「目标WIFI」「历史WIFI」 |
| 4 | 独立模拟 | `res/layout/cf.xml` | 页内「独立模拟应用」「打开独立模拟」 |
| 5 | GPS位置 | `cj.xml`（百度 MapView）/ `ck.xml`（Google SupportMapFragment） | 两页正文均为「当前GPS位置 / 纬度 / 经度 / 海拔 / 精度 / 速度 / 方向」 |
| 6 | 附近基站 | `res/layout/cg.xml` | 页内唯一标题「附近基站」 |
| 7 | 附近WIFI | `res/layout/cm.xml` | 页内唯一标题「附近WIFI」 |

7 个布局互不 include，也没有任何 Activity/Fragment 的 XML 引用，所以"抽屉项 → 页面布局"的绑定靠文案与控件语义推定；`setContentView` / `FragmentTransaction` 在 DEX 中，**未核验**。

### 1.4.1 页面模板（ci / cl / ch / cf / cg / cm 共用同一骨架）

```
LinearLayout(vertical, match_parent)
└─ NestedScrollView
   └─ LinearLayout(vertical, focusable+focusableInTouchMode, paddingBottom=24dp)
      ├─ [A] CardView 目标卡：margin 8/8/8/12dp，background=@color/a1e(#ffffff)
      │        app:hi=@dimen/fm(4dp)（推断 cardCornerRadius）
      │        app:m5=@dimen/fa(16dp)（推断 contentPadding）
      │        android:foreground="?a5g"（推断 selectableItemBackground 涟漪）
      │        └─ LinearLayout #<唯一 id>：标题(14dp, 绿) + 主值 + 副值 + 详情行 + 操作行
      ├─ [B] FrameLayout 区块头：TextView(14dp, @color/l7 灰)「历史位置/历史路线/历史WIFI/独立模拟应用」
      │        + 弹簧 View(weight=1) + ImageView #no（32dp, src=@mipmap/ad, tint=@color/l7）
      │        ← 这个箭头就是"查看全部/进入列表"
      └─ [C] FrameLayout
           ├─ include @layout/h0（#oh，**visibility=gone**，就是空状态占位）
           └─ CardView（margin 左右 8dp、下 24dp，app:hi=4dp）└─ LRecyclerView #<唯一 id>
```

**空状态** `res/layout/h0.xml` = `AppCompatImageView(@dimen/fz=80dp)` + 「点击右上角 "+" 按钮进行添加」（`@string/abb`）。它在 ci/cl/ch/cf 里都以 `#oh` + `visibility="gone"` 被 include：**空列表显示插画 + 提示，非空显示列表卡**。

### 1.4.2 位置模拟页 `ci.xml`（核心页，逐控件）

```
CardView(目标卡) └─ LinearLayout #ns
├─ TextView        14dp  @color/ix 绿        「目标位置」
├─ TextView #nx    20dp  bold  @color/l5     "NONE"（运行时填位置名）
├─ TextView #ny    14dp  bold                "NONE • NONE"（运行时填 省/市 或地址摘要）
├─ LinearLayout(horizontal)
│   ├─ TextView    「经纬度:」
│   └─ TextView #nz  14dp bold 绿            "30.4567876,104.5678765"
├─ LinearLayout(horizontal, gravity=center_vertical)   ← 操作行
│   ├─ carbon.LinearLayout #np  背景=@drawable/dg，高 28dp，
│   │      paddingLeft 16dp / paddingRight 8dp，圆角 14dp（推断）
│   │   ├─ carbon.TextView #o0  「启动模拟」14dp 白
│   │   └─ ProgressBar #nu  visibility=gone，accent=@color/iu(#f7cd1a 黄)
│   ├─ View  layout_weight=1（弹簧）
│   ├─ TextView #nw  「基站」14sp bold 绿
│   ├─ carbon.FrameLayout  30×30dp，圆角 15dp
│   │   └─ carbon.ImageView #nn  padding 5dp, src=@mipmap/a1, tint 绿  ← 展开 240dp「基站模拟」面板
│   └─ SwitchCompat #nv  「摇杆」14sp bold 绿                            ← 展开「摇杆」面板
└─ LinearLayout #nq  visibility=gone  ← 广告位（1dp 分隔线 + AdHtmlView）
区块头：「历史位置」+ 弹簧 + 32dp 箭头
列表：include #oh(空态, GONE) + CardView └─ LRecyclerView #nt
```

### 1.4.3 路线模拟页 `cl.xml`

```
CardView(目标卡) └─ LinearLayout #oo
├─ TextView  14dp 绿                    「目标路线」
├─ LinearLayout(horizontal, GONE)       ← 路线缩略行（默认隐藏）：24dp 路线图标 + 路线名
├─ LinearLayout(horizontal)             ← 起点/终点可视化列（**图标语义为推断**）
│   ├─ LinearLayout(vertical)：起点图钉 → 竖向虚线(weight=1, alpha=0.6) → 终点图标，全部 tint 绿
│   └─ LinearLayout(vertical, marginLeft 16dp)：「起点」(hint) / 「终点」(hint, marginTop 12dp)
├─ LinearLayout(horizontal, marginTop=16dp)   ← 操作行
│   ├─ carbon.TextView #os  「暂停」**硬编码**，GONE，高 28dp
│   ├─ carbon.LinearLayout #oj  绿色药丸 → carbon.TextView #ot「启动模拟」**硬编码** + ProgressBar(GONE)
│   ├─ View(weight=1)
│   ├─ TextView #ov  「速/频」14dp bold 绿   ← 弹出 240dp 小面板
│   ├─ carbon.FrameLayout(28×28dp) └─ ImageView #ok（tint 绿）
│   └─ TextView #ou  「循环」+ SwitchCompat #or
└─ LinearLayout #ol GONE  ← 广告位
区块头 + 列表：「历史路线」+ 箭头 + LRecyclerView #op
```

### 1.4.4 WIFI模拟页 `ch.xml`

```
CardView(目标卡) └─ LinearLayout #o4
├─ TextView 14dp 绿              「目标WIFI」
├─ TextView #o8  20dp bold       "NONE"（SSID）
├─ LinearLayout(horizontal)：「BSSID:」**硬编码** + TextView #o7 14dp bold 绿 "00:00:00:00:00:00"
├─ LinearLayout(horizontal)：绿色药丸 → 「启动模拟」**硬编码** + ProgressBar(GONE)
└─ LinearLayout #o2 GONE  ← 广告位
区块头 + 列表：「历史WIFI」+ 箭头 + LRecyclerView #o5
```

> id 复用：`#no`（查看全部箭头）与 `#oh`（空状态容器）在 ci/ch/cl/cf 中**同名复用**。

### 1.4.5 独立模拟页 `cf.xml`

```
CardView(目标卡) └─ LinearLayout #ng
├─ TextView 14dp 绿 = "TARGET"**硬编码英文**（运行时大概换成 目标位置/目标路线/目标WIFI）
├─ TextView #nj 20dp bold = "For Mock Location, Route, WiFi"（硬编码英文），hint="NONE"
├─ LinearLayout(horizontal, GONE)：包名行
├─ LinearLayout(horizontal, marginTop=8dp)   ← 操作行
│   ├─ 绿色药丸 → carbon.TextView #nm「打开独立模拟」+ ProgressBar(GONE)
│   ├─ View(weight=1)
│   ├─ carbon.TextView #nl  hint="+1"  背景 #f5f5f5，圆角 12dp，minWidth 24dp，GONE（多应用角标）
│   └─ ImageView(GONE, 24×24dp)
└─ LinearLayout #ne GONE  ← 广告位
区块头 + 列表：「独立模拟应用」+ 箭头 + LRecyclerView #nh
```

配套文案：`@string/ns`「开启独立模拟」、`@string/nt`「关闭独立模拟」、`@string/nv`「独立模拟已关闭」、`@string/nz`「提示: 独立模拟功能仅在ROOT模式下有效」、`@string/ck`「可设置位置模拟、路线模拟、WIFI模拟等功能仅对某一个或多个应用有效」、`@string/cl`「独立模拟 (ROOT)」。

### 1.4.6 附近基站 `cg.xml` / 附近WIFI `cm.xml`

两者结构完全相同，只是标题与列表 id 不同：

```
NestedScrollView → LinearLayout → CardView
├─ TextView 14dp 绿：「附近基站」/「附近WIFI」
├─ LinearLayout（广告位）
└─ LRecyclerView #oz（两张页面都叫 #oz）
```

配套文案：`@string/kn`「正在查询...」、`@string/q_`「正在搜索基站...」、`@string/qj`「正在搜索WIFI...」、`@string/km`「该地点附近暂无可用基站」、`@string/t9`「附近没有可用的WIFI信息」、`@string/t4`「未采集到可用的基站信息」。

### 1.4.7 GPS位置 `cj.xml`（百度）/ `ck.xml`（Google）

```
NestedScrollView → LinearLayout
├─ CardView(match_parent) └─ LinearLayout(vertical)
│   ├─ TextView 14dp「当前GPS位置」
│   ├─ 6 × LinearLayout(horizontal)：标签(bold) + 值（默认「获取GPS...」）
│   │      「纬度」「经度」「海拔」「精度」「速度」「方向」
│   └─ LinearLayout GONE（广告位）
└─ CardView → cj: com.baidu.mapapi.map.MapView（高 360dp）
             ck: com.google.android.gms.maps.SupportMapFragment（高 360dp）
```

配套文案：`@string/qh`「正在获取GPS...」、`@string/s0`「正在查询当前位置信息...」、`@string/rz`「查询位置信息错误:」、`@string/t5`「网络定位不可用」、`@string/t6`「SIM卡不存在」、`@string/t7`「定位权限被拒绝」、`@string/t8`「WIFI未启用」。

## 1.5 各功能界面清单

### 1.5.1 Activity 清单（取自 AndroidManifest，label 由 `@string` 反查）

| Activity | label | 布局（判定） | 依据 |
|---|---|---|---|
| `…ui.activity.MainActivity` | — | `a_.xml` + `ai.xml` + `b9.xml` | DrawerLayout + NavigationView + `@menu/c`，7 个目的页与抽屉一一对应；`setContentView` 在 DEX，未核验 |
| `…BaiduSelectLocationActivity` | 选择位置 | `ak.xml` | 百度 MapView + 4 FAB |
| `…BaiduSelectRouteActivity` | — | `al.xml` | 百度 MapView + 起终点搜索卡 + 出行方式 RadioGroup + 5 FAB |
| `…BaiduEditLocationActivity` | 位置编辑 | `aj.xml` | 百度 MapView + 名称/详细地址/纬度/经度/海拔 5 个 TextInputLayout |
| `…GoogleSelectLocationActivity` | 选择位置 | `a7.xml` | SupportMapFragment + 4 FAB |
| `…GoogleSelectRouteActivity` | — | `a8.xml` | 同上结构 + 5 FAB |
| `…GoogleEditLocationActivity` | 位置编辑 | `a6.xml` | SupportMapFragment + 5 个 TextInputLayout |
| `…AmapSelectLocationActivity` | 选择位置 | `ag.xml` | 高德 MapView + 4 FAB |
| `…AmapSelectRouteActivity` | — | `ah.xml` | 高德 MapView + 起终点卡 + RadioGroup + 5 FAB |
| （无 AmapEditLocationActivity） | — | 未确定 | manifest 里高德只有 SelectLocation / SelectRoute |
| `…SelectAppActivity` | 选择应用 | 未确定（疑为通用列表脚手架 `cn.xml`） | 全仓没有布局引用「搜索应用」/「选择应用」 |
| `…SelectWifiActivity` | 选择 WIFI | 未确定（疑为 `cn.xml`） | 同上，「请点击选择一个WIFI」不出现在任何布局里 |
| `…SettingsActivity` | 设置 | **`res/xml/g.xml`（PreferenceScreen）** | 见 §1.5.4 |
| `…RenewalActivity` | 专业版 | `ab.xml` + `dc.xml`/`d4.xml` | 页内有「专业版能力」「专业版套餐」、支付宝 |
| `…ProAnswerActivity` | 专业人士身份测试 | `aa.xml` + `h1.xml`/`h2.xml` | 字面量"同意协议并开始答题 / 倒计时 05:00 / 下一题 / 0分" |
| `…DeveloperIdentityAuthenticationActivity` | 认证开发者 | 未确定 | label「认证开发者」 |
| `…LoginActivity` | — | `cd.xml`（邮箱）/ `ce.xml`（手机号） | 见 §1.6 |
| `…BindPhoneActivity` | — | `cc.xml` | 见 §1.6 |

### 1.5.2 位置选择 / 位置编辑（三套地图）

**共同交互**：全屏地图 + 中央准心 + 右侧竖排 FAB 组（`app:vu="i4|a3x"` = 推断 bottom|end，靠右底对齐，用 `layout_marginBottom` 逐个错开 280/200/150/100/…dp）。

| 布局 | 地图 SDK | 顶/底卡片 | FAB 组 |
|---|---|---|---|
| `a7.xml` / `ag.xml` / `ak.xml`（三家的选择位置） | SupportMapFragment / 高德 MapView / 百度 MapView | 无 | 4 个 |
| `a8.xml` / `ah.xml` / `al.xml`（路线） | 同上 | CardView #c8（绿底、无阴影、圆角 4dp） | **5 个** |
| `a6.xml` / `aj.xml`（位置编辑） | SupportMapFragment / MapView | 顶部 5 个 TextInputLayout | 1 个 |

`a8.xml` 路线选择页的卡片内部（**路线规划的核心 UI**）：

```
FrameLayout
├─ SupportMapFragment #c9（全屏）
├─ ImageView #c7  居中的准心图标
└─ LinearLayout(vertical)
   ├─ CardView #c8  绿色卡：背景 @color/ix，无阴影，圆角 4dp
   │   └─ LinearLayout(vertical, clickable, marginTop 56dp, marginBottom 8dp)
   │      ├─ LinearLayout(horizontal)
   │      │   ├─ carbon.FrameLayout(40×40dp, 圆角 20dp) └─ 路线图标（白 tint）
   │      │   ├─ LinearLayout(vertical)  ← 起点/终点连线：起点图标 / 虚线(weight=1, alpha .6) / 终点图标
   │      │   └─ LinearLayout(vertical, weight=1)
   │      │        TextView #cb  hint=「搜索起点」，18dp，白字，hint 色 #f5f5f5
   │      │        TextView #ca  hint=「搜索终点」，同上，2dp 黄色光标
   │      └─ RadioGroup #ci（默认 GONE，gravity=center_horizontal）  ← 出行方式分段控件
   │           「驾车」(checked) / 「骑行」/「步行」
   │           三者：button=@null, background=@drawable/dh, textColor=@color/jt, 高 30dp
   │           @color/jt 是 selector：checked → #008577，否则 → #ffffff
   │           @drawable/dh 是 selector：checked → 白色全圆角药丸(radius 65dp)，否则 → 透明
   │           → **选中态 = 白底药丸 + 绿字；未选中 = 透明底 + 白字**（落在绿卡上）
   ├─ LRecyclerView #ce（白底）：搜索结果 / 途经点列表
   └─ View（高 6dp 占位）
```

FAB 组（`a8.xml`，从下往上 `marginBottom` = 28/100/150/200/280dp）：白底、荧光绿 tint，仅第一个是绿底。**图标字形无法确认（PNG）。**

相关文案：「拖动地图可继续规划路线」「请拖动地图和位置图标规划路线」「提示: 拖动地图和位置图标可自由规划路线；若需自动规划导航路线，可以在顶部依次搜索起点和终点」「请点击地图选择位置」「点击地图选择位置」「查询地点...」「搜索或输入经纬度」「选择位置」「完成」「位置编辑」「名称」「详细地址」「纬度」「经度」「海拔」「请输入经纬度」「纬度范围应为 -90～90，经度范围应为 -180～180」。

### 1.5.3 摇杆（`de.xml`）、基站面板、速度/步频面板、其它弹层

**摇杆 `de.xml`** 根节点是 CardView，内部一个 **`LViewSwitcher`（2 页）**：

- 第 1 页「设置」：摇杆方向（"Relative to Device" 硬编码英文，对应「相对于设备」/「相对于正北」）+ 1dp 分隔线；摇杆速度（EditText "10" + " m/s"）+ SeekBar + 5 个图标按钮的方向/速度预设；「摇杆锁定」+ Switch；「步频模拟」+ Switch；「步频」EditText "1" + "步/s"；「模拟GPS信号」+ Switch；「位置切换」行；「保存当前位置」行；底部「模拟」Switch + 「关闭摇杆」。
- 第 2 页「位置切换」：左右箭头 + 标题 + 位置列表。
- 文案：「摇杆功能需要您授予「悬浮窗」权限，点击下方按钮前往授权」「前往授权」「悬浮窗权限」「摇杆已锁定」「长按摇杆即可锁定摇杆方向」「摇杆服务正在运行」「通过摇杆控制位置往任意方向上的移动，摇杆中支持自定义移动速度、锁定移动方向、快速切换位置、记录当前位置等功能」。

**基站模拟面板 `c7.xml`**（**240dp 固定宽的锚定小面板，不是整页**）：

```
LinearLayout(vertical, width=240.0dp, paddingTop/Bottom=4dp)
├─ LinearLayout(horizontal)：「基站模拟」+ 弹簧 + Switch #kz
└─ LinearLayout(horizontal, id=#ky, clickable, foreground=涟漪)
     「基站列表」+ 弹簧 + TextView #l0(text="0", hint="正在查询..", 绿) + ImageView(20dp, 刷新图标, tint 绿)
```

文案：「基站模拟 (ROOT)」「模拟目标位置附近的基站数据…」「该位置是采集的位置, 查询基站列表操作会覆盖其采集的基站信息，确定要继续查询基站列表吗?」「查询异常」「该地点附近暂无可用基站」「正在查询...」「基站模拟已停用」「基站模拟已启用」「提示: 基站模拟仅在ROOT模式下有效」「注: 如果目标APP仅使用的基站(网络)定位, 那么还需开启"基站模拟"才有效」。

**速度/步频面板 `c9.xml`**（**240dp 宽弹出面板**，由路线页「速/频」按钮打开）：

```
LinearLayout(vertical, width=240.0dp, padding 10dp/8dp)
├─ 「运动速度」+ EditText #l6(text="10", 右对齐, 绿) + "m/s"
├─ SeekBar #l8  max=40, progress/thumb/background 全绿
├─ 5 个 16×16dp 图标（速度预设，tint 绿），中间弹簧分隔
├─ 「速度浮动」+ 弹簧 + Switch #l9
├─ 「步频模拟」+ Switch #l7
└─ 「步频」+ EditText(text="1", 右对齐绿) + "步/s"
```

文案：「步频模拟最大仅支持50步/s」「提示: 点击速度值可自由输入速度值」「提示: 步频模拟仅在ROOT模式下有效」「未在该设备上识别到方向、加速度或计步等传感器，步频模拟可能无效！」「提示: 步频值若超过设备传感器接受值会导致模拟失效，建议根据实际情况设置合理值」「步频模拟 (ROOT)」「计步器每日清零」。

**其它功能面板 / 弹窗**：

| 布局 | 功能 | 关键控件与中文文案 |
|---|---|---|
| `c8.xml` | 路线循环参数弹窗 | 「循环次数」+ EditText "2" +「次」；「循环间隔」+ EditText "0" +「秒」 |
| `c4.xml` | 路线命名弹窗 | 输入框 hint「请输入路线名」 |
| `c5.xml` | 计步器弹窗 | 「系统计步器步数:」+「未检测到计步器」；「设置计步器步数」 |
| `br.xml` | 添加/编辑 WIFI 弹窗 | 「WIFI 名称」「WIFI BSSID」 |
| `bx.xml` | 位置采集弹窗 | 「采集GPS」「采集基站」「采集WIFI」+「名称: 」+ 提示 |
| `c3.xml` | 采集实时位置（带地图） | 「采集实时位置」+ 同上 |
| `bs.xml` | 位置信息 / S码分享内容块 | 「位置信息」+ 名称 + "NONE • NONE" +「经纬度:」；「附加信息」+「基站信息」「WIFI信息」 |
| `gw.xml` / `gx.xml` | 导入位置（S码/链接/经纬度） | 「支持 经纬度、S码、地点链接...」「• 经纬度: 支持 WGS84 十进制度坐标… • S码: … • 地点链接: …」「导入位置」「解析」「从本地文件导入」 |
| `bz.xml` | 外部调用 API 说明 | 「重置KEY」「使用教程」+ ADB 命令帮助（`fl.mock.start/set/stop` 广播） |
| `cn.xml` | **通用列表页脚手架**（推断用于 选择应用 / 选择 WIFI / 设置等） | 根 FrameLayout + 居中空态占位 + 顶部条容器 + SwipeRefreshLayout > LRecyclerView + 底部条容器 + FAB(**visibility=gone**)。**子容器都是空的，内容全部代码填充 → 归属哪个 Activity 未确定** |

### 1.5.4 列表行布局

| 布局 | 用途 | 结构 |
|---|---|---|
| `cv.xml` | 历史/搜索结果行 | 40×40dp 绿色圆底 + 图标 + 名称(bold 16dp) + 「经纬度:」+ 值 + 右侧 24dp ImageView(GONE) |
| `cu.xml` | 专属包下载行 | 图标框 + 名称 + 副文本(NONE) + "打包排队中... 2/10" + 竖分隔线 + 绿色「下载」按钮 + ProgressBar(GONE) |
| `d8.xml` / `d_.xml` | 1dp 横分隔线 / 1px 竖分隔线 | 背景 `@color/j5`(#f2d4d4d4) |
| `h0.xml` | 空状态 | 80dp 图片 +「点击右上角 "+" 按钮进行添加」 |
| `gz.xml`（推断） | 专属包空状态 | 「点击右下角 "+" 按钮定制专属包」 |

### 1.5.5 设置页 = PreferenceScreen（`res/xml/g.xml`，完整还原）

`SettingsActivity` 不用普通 layout，而是 AndroidX Preference 框架。`PreferenceScreen android:key="ps_root"`，标题/摘要均为中文原文，键名保留原始英文：

| 分类 | 项 | 类型 | key | 默认值 | 标题 | 摘要 |
|---|---|---|---|---|---|---|
| — | 默认语言 | ListPreference | `default_language` | `auto` | 默认语言 | Select the default language for the app（英文硬编码） |
| **位置模拟** | 默认地图 | ListPreference | `general_default_map` | 0 | 默认地图 | — |
| | 位置更新频率 | EditTextPreference | `mock_interval_timeout` | 100 | 位置更新频率 (毫秒/1次) | — |
| | 模拟GPS精度 | EditTextPreference | `mock_gps_accuracy` | 1.2 | 模拟GPS精度 (米) | — |
| | 模拟GPS浮动 | Switch | `mock_gps_float_enable` | true | 模拟GPS浮动 | 在设置的GPS精度范围内启用经纬度坐标浮动 |
| | 模拟GPS信号 (ROOT) | Switch | `mock_gps_status_enable` | true | 模拟GPS信号 (ROOT) | 在位置模拟和路线模拟的同时启用GPS信号(强度)模拟… |
| | 允许搜索GPS信号 (NOROOT) | Switch | `mock_allow_search_gps_enable` | false | 允许搜索GPS信号 (NOROOT) | 允许设备在NOROOT模式下…不建议开启 |
| | 外部调用API | Switch | `is_mock_api_enabled` | false | 外部调用API | 使用命令行或外部应用控制模拟位置 |
| **基站模拟** | 模拟SIM卡信息 (ROOT) | Switch | `mock_siminfo_enable` | false | 模拟SIM卡信息 (ROOT) | 在模拟基站的情况下同时模拟SIM卡信息… |
| **步频模拟** | 计步器每日清零 | Switch | `mock_steps_day_reset_enable` | false | 计步器每日清零 | 每日自动重置计步器，从 0 开始计步 |
| **多平台** | X86平台组件 | Switch | `abi_x86_enable` | false | X86平台组件 | 启用该组件后可在x86平台(例如:电脑端的安卓模拟器)上使用ROOT模式 |
| **调试** | 系统就绪时长 | EditTextPreference | `sysready_time` | 180000 | 系统就绪时长(毫秒) | 系统开机到完全就绪的时长(建议值: 180000) |
| | SELinux | Switch | `selinux_status` | true | **SELinux**（硬编码） | **Unknown**（硬编码） |
| | SELinux自动恢复 | Switch | `selinux_auto_enforcing` | true | SELinux自动恢复 | 功能使用完毕后自动恢复 SELinux 为启用状态(强制模式) |
| | Xposed兼容 | Switch | `xposed_mode` | false | Xposed兼容 | 在安装有Xposed框架的情况下建议启用该选项… |
| | 日志记录 | Switch | `is_logcat_enabled` | false | 日志记录 | 捕获运行日志，可通过反馈通道发送给开发者… |
| | 清理运行环境 | Preference | `clearEnv` | — | 清理运行环境 | 若升级版本后出现运行异常, 可尝试清理一下运行环境… |
| **其它** | 历史记录大小 | EditTextPreference | `history_size` | 100 | 历史记录大小 | 最多支持300条历史记录 |
| | 数据导出 / 数据导入 | Preference | `data_export` / `data_import` | — | 同名 | 导出/导入数据 |
| **关于** | 版本号 | Preference | `versionCode` | — | 版本号 | V1.0.0（硬编码示例） |
| | 用户协议 / 隐私政策 | Preference | `userAgreement` / `privacyPolicy` | — | 同名 | 点击查看 |
| | 联系开发者 | PreferenceScreen | `contact` | — | 联系开发者 | Email: lerist.5@gmail.com（硬编码） |

### 1.5.6 运行模式（`@string/ng`）

- **没有对应布局文件**：全仓 layout 检索不到「运行模式」「NOROOT 模式」「ROOT 模式」「切换模式」的任何引用，推断是**代码构建的对话框/列表**。**未确定**。
- 相关文案：「第一次运行，请先设置运行模式」「仅支持部分功能, 且对基站、WIFI定位方式无效」「支持全部功能, 需设备具有ROOT权限，并且需要"通过专业人士身份测试"以及"开通专业版"」「运行模式已切换, 请重新开启位置模拟」「NOROOT模式下，需要允许程序模拟位置，请依次前往: 设置 - 开发者选项 - 选择模拟位置信息应用…」「ROOT模式: 需要设备具有并授予程序ROOT权限，且系统内核支持切换SELinux为Permissive(宽容模式)…」。

### 1.5.7 登录 / 绑定手机号（`LViewPager` 真正的用途）

| 布局 | 页面 | 步骤标题器 | 步骤表单 | 底部 |
|---|---|---|---|---|
| `cc.xml` | 绑定手机号 | 2 页 | 2 页：手机号 + 国家代码 → 验证码 | ProgressBar（**不含协议勾选**） |
| `cd.xml` | 邮箱登录 | 5 个子项 | 5 页：邮箱 → 验证码 → 密码 → 设置密码 → 完成页「欢迎」 | ProgressBar + 协议行 |
| `ce.xml` | 手机号登录 | 5 个子项 | 结构同 cd，首项换成手机号 + 国家代码 | 同 cd |

`dg.xml`（协议行）= CheckBox +「我已阅读并同意」+《用户协议》+「和」+《隐私政策》。交互要点：`LViewPager` 每一页 = 一个向导步骤，`LViewSwitcher` 联动显示"大标题(28dp) + 说明"，卡片区 `marginTop 84dp`，`LViewPager` 设了 `minHeight 600dp`。

## 1.6 关键交互模式（只写有依据的）

| # | 模式 | 依据 | 可信度 |
|---|---|---|---|
| 1 | **开关几乎不整行可点，而是"左标签 + 右 Switch"两栏行** | `c9.xml`、`c7.xml` 第一行、`de.xml` 的摇杆锁定/步频模拟/模拟GPS信号 | 高 |
| 2 | **少数行是真"整行可点"**：`c7.xml` 的"基站列表"行有 `foreground` 涟漪 + 右箭头；FAB 与"解锁专业版"有 `clickable` | 高 |
| 3 | **区块头右侧箭头 = "进入列表/查看全部"** | 4 个主页面共用：灰 14dp 标题 + 弹簧 + 32dp 箭头；`@string/f5`（`values-zh-rCN`）=「查看全部」 | 高（结构）/ 中（文案归属） |
| 4 | **主操作是"绿色圆角药丸按钮 + 内嵌转圈"** | `@drawable/dg` selector（disabled `#a4008577` / enabled `#008577`），高 28dp，圆角 14dp，内含白字 14dp + ProgressBar（accent 黄 `#f7cd1a`）；出现在 ci/cl/ch/cf 四页 | 高 |
| 5 | **有悬浮按钮，地图页是"竖直 FAB 组"** | 主界面 FAB 锚在内容容器上、自定义滚动行为；地图页 4~5 个 FAB 按 marginBottom 错开成右下一列 | 高 |
| 6 | **参数类操作用"锚定小面板"而不是整页** | `c7.xml` / `c9.xml` 根布局宽 240dp——固定 240dp 不可能铺满屏幕，只能是 PopupWindow/锚定弹层；触发点是位置页的"基站"图标与路线页的"速/频"文字 | 高 |
| 7 | **"目标卡 + 区块头 + 列表"三段式是主界面统一版式**，空态共用同一插画+文案 | `h0.xml` | 高 |
| 8 | **分段选择器用 RadioGroup + `button=@null` + selector 背景** | 选中 = 白色全圆角药丸(radius 65dp) + 绿字；未选中 = 透明 + 白字 | 高 |
| 9 | **卡片风格**：白底 + 4dp 圆角（推断）+ 无阴影，页面底色 `#fafafa`；地图页卡片是绿色 | 高（`app:hi` 语义属推断） |
| 10 | **没有底部弹窗**：全仓检索 `BottomSheet\|BottomAppBar\|ChipGroup\|TabLayout` **零命中** | 高（"未使用"是检索结论） |
| 11 | **功能页内没有分组标题**：只有单个区块头，唯一的"分类标题"出现在设置里 | 高 |
| 12 | **列表项右侧箭头 → 未找到依据** | `cv.xml` 右侧只有一个默认 GONE 的 24dp ImageView | — |
| 13 | **抽屉是 NavigationView + 三组可折叠子菜单**，4 个主项单选高亮 | 高 |
| 14 | **页面切换动画未确定**：`res/anim`(58)、`res/animator`(34)、`res/interpolator`(18) 全为混淆名 | — |

## 1.7 配色、圆角、间距、字号

### 1.7.1 主题链（可确认部分）

```
@style/l（application theme）
└─ 链上最终父主题 = @android:style/Theme.Material.Light.NoActionBar
    style/dw: android:colorPrimary=?kv  colorPrimaryDark=?kx  colorAccent=?k0
    style/l : kv=#008577  kx=#007266  k0=#008577
    style/l : android:statusBarColor=#007266
→ 结论：**浅色 Material 主题**，colorPrimary=#008577、colorPrimaryDark/状态栏=#007266、colorAccent=#008577。
```

其它：抽屉头主题 `@style/cs` 的 `windowBackground=#ff303030`（深色，衬 centerCrop 图）；`@style/r`（药丸内转圈）的 accent = `#f7cd1a`。

### 1.7.2 实际色值（全部来自 `res/values/colors.xml`）

| 语义 | 资源 | 色值 | 使用位置 |
|---|---|---|---|
| **主色 / 品牌绿** | `@color/ix`、`@color/it` | **#008577** | 卡片标题文字、图标 tint、按钮背景、colorPrimary/Accent |
| **深主色 / 状态栏** | `@color/iy` | **#007266** | statusBarColor、colorPrimaryDark |
| 主色 64%（禁用态） | `@color/iz` | #a4008577 | 药丸按钮 disabled、地图页 FAB tint |
| **页面背景 / 抽屉背景** | `@color/j3` | **#fafafa** | DrawerLayout 与 NavigationView background |
| **卡片 / 表面** | `@color/a1e` | **#ffffff** | 所有 CardView background、主色上的文字 |
| 抽屉头主题背景 | `@color/a4`→`@color/sv` | #ff303030 | `@style/cs` 的 windowBackground |
| **正文文字** | `@color/l5` | **#4e6161** | 卡片名称、设置项文字 |
| **次要文字 / 区块头 / 箭头** | `@color/l7` | **#a1a1a1** | 「历史位置」标注、箭头 tint |
| 输入框 hint / FAB 角标字色 | `@color/l_` | #757575 | |
| hint（绿卡上） | `@color/l9` | #f5f5f5 | 路线页搜索框 hint、计数背景 |
| **分隔线** | `@color/j5` | **#f2d4d4d4** | `d8.xml` / `d_.xml` |
| 涟漪色 | `@color/y2` | #464f504f | carbon `app:g9` |
| 图标默认 tint | `@color/lj` | #222222 | `menu-v22/c.xml` |
| **强调黄（进度）** | `@color/iu` | **#f7cd1a** | 药丸内 ProgressBar accent、搜索光标 |
| 警告红（未定位使用处） | `@color/gw`/`gx` | #b71c1c / #ff8a80 | 第三方库，用途未确定 |
| 动态色相关 | `@color/a3p`/`a4c`/`a5g` | v31/v34 系统色 | 部分控件（SwitchCompat）接了 Material3 动态色，具体映射未确定 |

### 1.7.3 圆角与间距（dimens 名字同样混淆，但数值可读）

| dimen | 值 | 典型用途 |
|---|---|---|
| `fm` | **4dp** | CardView 圆角（推断）、小内边距、分隔线上下距 |
| `fa` | **16dp** | 左右内边距、图标尺寸、FAB 尺寸 |
| `ff` | **24dp** | 区块左右边距、图标尺寸 |
| `fy` | **8dp** | 卡片外边距（8/8/8/12）、行内水平间距 |
| `f6` | **12dp** | 卡片下边距、角标圆角 |
| `f3` | **10dp** | 面板内边距、小字 |
| `fc` | **2dp** | 抽屉"解锁专业版"按钮圆角（推断） |
| `f9` | **14dp** | 正文/按钮文字、"启动模拟"药丸圆角 |
| `f5` | **11dp** | 小号文字 |
| `f_` | **15dp** | 站点图标圆角（路线页 28dp 容器） |
| `fd` | **20dp** | 卡片主名称字号、抽屉头应用名字号 |
| `fb` | **18dp** | 路线页名称/起终点字号 |
| `fh` | **28dp** | 药丸按钮高度、站点容器 |
| `fj` | **30dp** | 分段按钮（驾车/骑行/步行）高度 |
| `fk` | **32dp** | 区块头箭头尺寸 |
| `fn` | **40dp** | 列表行图标容器 |
| `fq` | **48dp** | 空态图片上边距、输入框区域 |
| `ft` | **56dp** | 地图页卡片顶部偏移 |
| `fz` | **80dp** | 空态插画尺寸 |
| `bj` | **300dp** | AppBarLayout（CollapsingToolbar）高度 |
| `wf` | **220dp** | 抽屉头 minHeight |
| `g2` | **32dp** | 主界面 FAB margin |
| — | **240dp** | 锚定面板固定宽度（`c7.xml`/`c9.xml` 显式写 `240.0dp`，**非推断**） |
| — | **65dp** | 分段选中药丸圆角（`@drawable/dd` 显式，**非推断**） |

**字号单位不统一（实测）**：同一个 14 有 dp/sp 两种写法——`@dimen/f9`=14.0dp（卡片标题、按钮文字、区块头），`@dimen/xl`=14.0sp（"基站"标签、"摇杆"开关文字）；另有 `@dimen/xc`=20.0sp。Web 端统一按视觉值取 14px 级别即可。

### 1.7.4 拿不到的部分（明确说明，不要当成事实）

1. `attrs.xml`（91 KB）里所有自定义/库属性名都被混淆，**没有反向映射表**。因此"`app:hi` 到底是不是 cardCornerRadius"**无法证实**——只能据上下文（CardView 上只出现 `app:hg`/`app:hh`/`app:hi` 三个属性，且 `hg` 的值是颜色、`hh` 是 `0.0dp`）推断为 cardBackgroundColor/cardElevation/cardCornerRadius。同理 `app:eh`（carbon 圆角）、`app:g9`（涟漪）、`app:a73`（srcCompat）、`app:vt`/`app:vu`（anchor/anchorGravity）等均为推断；`app:vv` = `layout_behavior` 是唯一被证实的。
2. `styles.xml`（290 KB）里**没有** `shapeAppearance*` / `cornerSize` / `cornerFamily` 的原文名（检索零命中），所以 Material 的 shape 体系无法从资源读出；本文的"圆角"只来自具体控件的具体属性值。
3. 夜间模式：存在 `values-night/colors.xml`，但只覆盖一批本文未用到的色名；**主色/背景色是否有夜间变体未确定**（`@color/j3`、`@color/a1e`、`@color/ix` 在 values-night 中均无覆盖）。
4. 图标字形（多为 PNG 位图）、`app:cf="80.0dp"` 的语义、页面切换动画、几个只含 `LFragmentContainer` + FAB 的容器布局归属，均**未确定**。

## 1.8 对重做 Web 界面最有参考价值的 5 条交互习惯

1. **"三段式主页"**：顶部目标卡（大名称 + 副信息 + 主操作药丸，右上角挂功能入口）→ 中间灰色区块头 + 右侧"查看全部"箭头 → 底部白卡里的历史列表。四个主功能页（位置/路线/WIFI/独立）**完全同构**，Web 端可用一个组件复用。
2. **主操作永远是"绿色（#008577）药丸 + 内嵌 loading"**：高 28dp、圆角 14dp、白字 14sp，禁用态 64% 透明绿；点击后原地点亮转圈（黄 #f7cd1a），不做页面跳转或遮罩。
3. **开关行 = "左标签 + 弹簧 + 右 Switch"**，行本身不整行可点；只有少数行（"基站列表"）整行可点并带右侧图标。参数类操作走**锚定小面板/弹窗**，其中"基站"和"速/频"固定 **240dp 宽**。
4. **状态用"同一个卡片换态"而不是新页面**：空列表 → 80dp 插画 +「点击右上角 "+" 按钮进行添加」；未启动 → NONE 占位 +「启动模拟」；启动中 → 按钮内转圈；运行中 → 按钮文案切到"停止模拟/暂停"。
5. **分段选择器用"白色全圆角药丸 + 绿字"表示选中**（未选中为透明底 + 白字，落在绿色卡片上），可 1:1 复刻。

---

# 二、Miuix（米 UI）外观参考

来源是 KernelSU 管理器的实际用法（`.deepseek/references/kernelsu`）。**Miuix 是第三方库**（`top.yukonga.miuix.kmp` 0.9.3），不是该仓库自己的实现，仓库只是调用它；**凡仓库里看不到的值一律写"未找到依据"，不用通用知识编造。** 该仓库里 Miuix 是默认外观（`ui/UiMode.kt:15,19`），每个页面有三份文件（分发 / Miuix / Material），用 `when (LocalUiMode.current)` 分流。

> 关于 `SuperSwitch` / `SuperArrow` / `SuperDialog` / `SuperDropdown`：全仓 grep **未找到**这些标识符。仓库内唯一带 `Super` 前缀的自研组件是 `SuperSearchBar` 与 `SuperEditArrow`（都是对 miuix 基础组件的封装）。对应的**真实 miuix 组件名**是 `SwitchPreference`、`ArrowPreference`、`WindowDialog`/`OverlayDialog`、`OverlayDropdownPreference`。

## 2.1 配色

**颜色不是"自带一套固定色板"，而是三层结构**：用户设置（KeyColor / Monet / PaletteStyle / ColorSpec）→ 种子色 → materialkolor 生成色板 → `MiuixTheme(controller=...)` → `colorScheme.*`。

- 种子色三选一（`ui/theme/MiuixTheme.kt:45-52`）：用户选的 15 个关键色之一 / Monet（**只取 `dynamicXxxColorScheme(context).primary` 当种子**，不是直接用系统色板）/ 交给 miuix 内置默认色板。关键色实值在 `ui/theme/Colors.kt:6-22`。
- 默认 `colorSpec = SPEC_2025`（仅 `TonalSpot / Neutral / Vibrant / Expressive` 支持，其它风格降级为 Spec2021）；`paletteStyle` 映射失败回落 `TonalSpot`。
- 深浅色判定：`appSettings.colorMode.isDark || (isSystem && isSystemInDarkTheme())`；AMOLED 在 Miuix 路径下映射为 `MonetDark`。

**仓库内确实出现过的颜色 token**（可直接映射为 CSS 变量名）：`surface`、`surfaceVariant`（卡片底色）、`surfaceContainer`（悬浮底栏/关于页卡片）、`surfaceContainerHigh`（搜索框底）、`surfaceContainerHighest`（预览卡）、`onSurface`、`onSurfaceVariantSummary`（**次要说明文字，出现频率最高**）、`onSurfaceVariantActions`（行尾动作文字/图标）、`onSurfaceSecondary`（第三级文字）、`onSurfaceContainer`（导航选中态；未选中 = 同色 alpha 0.5）、`onBackground`（默认内容色/行首图标 tint）、`primary`/`onPrimary`、`primaryContainer`/`onPrimaryContainer`、`secondaryContainer`、`tertiaryContainer`、`error`/`errorContainer` 系列、`outline`（分隔线/描边，惯例 `copy(alpha=0.5f)`）、`disabledOnSecondaryVariant`。此外还有 5 个只在 CSS 桥里出现的：`primaryVariant`、`tertiaryContainerVariant`、`dividerLine`、`windowDimming`、`disabledOnSurface`。

**唯一可见的具体色值**（`isDynamicColor == false` 时的兜底，仅用于警示/状态卡，**不是通用卡片或页面底色**）：

| 用途 | 深色 | 浅色 |
|---|---|---|
| 警示卡片底（Error） | `0xFF310808` | `0xFFF8E2E2` |
| 警示卡片底（Notice） | `0xFF3E2F1B` | `0xFFFFF0DB` |
| 警示文字（Error / Notice） | `0xFFF72727` / `0xFFF5A623` | 同左 |
| 状态强调卡片底 / 图标 | `0xFF1A3825` / `0xFF36D167` | `0xFFDFFAE4` / 同左 |
| LKM 徽标 bg / fg | `0xFF315D3E` / `0xFFB8E8C5` | `0xFFB8E8C5` / `0xFF164A29` |

> ❗ **未找到依据**：`surface`、`onSurface`、`primary` 等 token 在 Miuix **内置非动态色板**下的 RGB，以及任何动态派生后的实际色值——全部由库内部计算。Web 复刻应把上表当"已知常量"，其余当"需要从 Android 端导出或按 miuix 规范补齐的未知量"。

## 2.2 CSS 变量桥（对 Web 复刻最有价值的一块）

**该仓库自己就把色板映射成了 CSS 自定义属性**（`ui/webui/MonetColorsProvider.kt:36-90`，`UpdateCssMiuix`）。这是"Web 端该用什么变量名"的**权威依据**，不需要重新发明命名。导出格式为 `:root { --primary: #rrggbb; ... }`，`alpha != 1f` 时写 `#rrggbbaa`。

Miuix → CSS 映射里**语义发生偏移、值得注意**的几条：

| CSS 变量 | Miuix token |
|---|---|
| `--inversePrimary` | `primaryVariant` |
| `--tertiary` | `tertiaryContainerVariant` |
| `--onTertiary` | `tertiaryContainer` |
| `--tonalSurface` | `surfaceContainer` |
| `--onSurfaceVariant` | `onSurfaceVariantSummary` |
| `--inverseSurface` | `disabledOnSurface` |
| `--inverseOnSurface` | `surfaceContainer` |
| `--outlineVariant` | `dividerLine` |
| `--scrim` | `windowDimming` |
| `--surfaceBright` / `--surfaceDim` | `surface`（同 surface） |
| `--surfaceContainerLow` / `--surfaceContainerLowest` | `surfaceContainer` |
| `--filledTonalButtonContentColor` / `ContainerColor` | `onPrimaryContainer` / `secondaryContainer` |
| `--filledCardContentColor` / `ContainerColor` | `onPrimaryContainer` / `primaryContainer` |

其余同名直通（`--primary`、`--onPrimary`、`--primaryContainer`、`--secondary`、`--background`、`--onBackground`、`--surface`、`--onSurface`、`--surfaceVariant`、`--error`、`--outline`、`--surfaceContainerHigh/Highest` 等），共 46 个变量。**Material 侧用的是同一套变量名但填 M3 原生 token**（例如 `--tonalSurface` = `surfaceColorAtElevation(1.dp)`、`--scrim` = `scrim`），因此同一个变量名在两套外观下语义可以不同；只做 Miuix 时必须用 Miuix 那一份映射。

> 该导出函数的**消费端不在仓库里**（grep 这些变量名 0 匹配），显然是注入给模块 WebUI 的，但消费端 CSS 未随仓库提供。

## 2.3 形状、尺寸、排版

**页面级间距（全部有代码依据）**：列表水平内边距 **12dp**、卡片之间垂直间距 **12dp**、首张卡片距顶栏 **12dp**、末张卡片距底 **12dp**、卡片内部内边距 **16dp**、卡片内首末行上下留白 3dp、行首图标与文字间距 **6dp**（约 20 处一致）、信息行图标 24dp + 右侧 12dp、信息行之间 24dp。

**组件内尺寸**：`EditText` 内边距 16dp / 最小高 56dp；`DropdownItem` 水平 20dp（首项 top 20、末项 bottom 20、中间 12）；`WarningCard` 内边距 16dp / 文字 14sp；搜索框 `heightIn(min=45dp)` + 全圆角 + 水平 12dp，搜索图标占位 44dp；分隔线 **0.5dp**（排序菜单里 1.5dp），低对比 `outline.copy(alpha=0.5f)`，上下留 8dp；细描边 0.05dp；小图标按钮 min 35×35dp。

**可见的圆角值**：徽标 6dp、模块图标 6dp、应用图标 12dp、图标容器/内嵌卡片 16dp、大圆按钮 25dp、顶部通栏图 0dp、小强调按钮 cornerRadius 50dp。

> ❗ **未找到依据**：`Card`、`WindowDialog` / `OverlayDialog`、列表行的**圆角**，以及开关的形状与尺寸——仓库里所有 `Card(...)` / `WindowDialog(...)` / `OverlayDialog(...)` 调用**都没有传 `shape`**，完全由库默认值决定。Web 复刻必须从 miuix 规范或实机取值，不要自行假设。**唯一例外**是主题预览用的手绘 mock（机身 20dp、卡片 6dp、侧栏项 3dp、悬浮胶囊 14dp/高 28dp、侧栏宽 30dp 等），那是**示意图，不可当作真实组件度量**。

**悬浮底栏（真实组件，值可信）**：全圆角胶囊，容器色 `surfaceContainer`（模糊开启时 0.4 透明度），高 **64dp** + 内边距 4dp（内容 56dp），投影 radius 10dp / alpha 深 0.2 浅 0.1，选中指示器 `primary` 15% 透明胶囊，页面左右外边距 28dp、距底 28dp，单 tab 最小宽 76dp，标签 11sp / 行高 14sp，按压缩放 0.78/0.56、选中放大 1.2，模糊 `blur(4.dp)` + `lens(24.dp)` + `vibrancy()`。顶栏/底栏/侧栏的模糊底衬统一 `textureBlur(blurRadius=25f, BlendColorEntry(surface.copy(0.87f)))`，模糊生效时栏底色改为透明。

**其余可靠尺寸**：分组起始 top 12dp；行尾图标箭头左侧间距 **6dp**（32 处统一）；对话框内行首图标右侧 16dp；卡片行内图标 24dp（信息行）/ 40dp（行首大图标）/ 20dp（行内动作）；行内小箭头 10×16dp；动作胶囊最小 35×35dp；FAB `shadowElevation = 0.dp`；`TabRow` 高 40dp；弹窗列表 `heightIn(max=500dp)`；下拉列表 `maxHeight = 280dp`。

**排版**：Miuix 侧**排版完全由库决定**，仓库只读 `MiuixTheme.textStyles.X.fontSize`（出现过的 token：`title4` 对话框标题、`headline1` 行主标题、`body1` 下拉项、`body2` 行副标题、`main` 输入框、`footnote1` 脚注）——这些 token 的**字号/行高/字重未找到依据**，不要在 Web 里自行假定。

显式书写的字号可信：状态卡大标题 22sp SemiBold、模式标签 16sp Medium、版本行 15sp；模块名 17sp FontWeight(550)；文件名/作者/警示卡/说明 14sp；源码链接与离线提示 16sp；搜索框 17sp Medium；状态徽标 9sp FontWeight(750)；悬浮底栏标签 11sp。**全量扫描的 sp 集合**只有 `9/11/12/13/14/15/16/17/22/35`，其中 **12sp 用得最广（约 30 处）**，13sp 与 35sp、22sp 各只有 1 处。**字重取值**：`Medium` 最多，`FontWeight(550)`（正文级强调，数十处）、`FontWeight(750)`（徽标级），`SemiBold` 1 处、`Bold` 2 处。层级经验：大标题 22 → 行标题 17/16 → 正文 15/14 → 说明 14（`onSurfaceVariantSummary`）→ 徽标 11/9。Web 对应：`Medium` = 500，`550`/`750` 是可变字重的非整百值，CSS 可直接写 `550`/`750`。

## 2.4 页面骨架与交互特征

**标准骨架**：`Scaffold(topBar = 模糊栏 + TopAppBar, contentWindowInsets = systemBars+displayCutout 的**只水平方向**)` → `Box` → `LazyColumn(padding(horizontal = 12.dp), contentPadding = innerPadding, overscrollEffect = null)`，页面内容常放在**单个 `item { }`** 里、内部是若干 `Card`。栏底色统一写法 `if (blurActive) Color.Transparent else colorScheme.surface`（Miuix 侧不用 `containerColor`）。

**列表分组**：**分组 = 一张 Card**，组间 `padding(top = 12.dp)`；设置类卡片内部**不使用分隔线**（`SwitchPreference` 直接连续堆叠）；需要时才加 0.5dp 低对比分隔线。**分组标题用 `SmallTitle`，位于 Card 之外、Card 之上**（全仓库只有 3 处使用），**设置页完全没有分组标题**。另一种"手写分组标题"是 13sp / Medium / `onSurfaceVariantSummary` / 水平 20dp + 垂直 6dp——**Web 复刻若要"小号灰色分组标题"应以此为准**。

**卡片与页面**：页面底色 = `Scaffold` 默认（Miuix 未指定）；卡片底色 = `Card` 默认（非动态色板下为 `surfaceVariant`）；卡片左右外边距 12dp、间距 12dp、内部 16dp；卡片可整卡可点（`onClick` + `showIndication` + `pressFeedbackType`）。

**底部栏 / 侧栏**：根导航是 4 页 `HorizontalPager`（Home / SuperUser / Module / Setting）。底栏两种形态——贴边 `NavigationBar`，或用户可开的**悬浮胶囊** `FloatingBottomBar`（居中、**仍占位、不覆盖内容**）。侧栏 `NavigationRail` 可展开/收起并持久化；切换条件 `shouldShowSplitPane() && !(uiMode == Miuix && enableFloatingBottomBar)`；响应式断点 `widthDp >= 840f || (widthDp >= 600f && heightDp / widthDp < 1.2f)`。非全功能版本直接不渲染底栏/侧栏。

**空状态**：统一是**居中一行灰字**（或居中标题+副标题、或"一行文字 + 重试按钮"），**不用插图卡片，仓库内没有统一空状态组件**。

**底部留白**：设置页 `Spacer(bottomInnerPadding)`；首页 `bottomInnerPadding + 导航栏高度`；主题页 `navigationBars + captionBar + 12.dp`。

**交互特征**：① 开关**整行可点**（`SwitchPreference` 只收 `checked` + `onCheckedChange`，开关在行尾，行首可选图标），Material 侧同样整行可点并带触感；② 箭头行**自带箭头**，并支持 `endActions`（右侧文本如 "100%"）与 `bottomAction`（展开后在行下方渲染 Slider）；③ 滑块是"吸附 + 步骤触感"式（`showKeyPoints` + `magnetThreshold`）；④ 下拉是覆盖层列表，`items` 为纯字符串；⑤ 按压反馈有类型（`Tilt` / `Sink`）；⑥ 所有列表统一滚动收尾触感 + 关闭 overscroll 光晕；⑦ **对话框按钮等宽并排**（各 `weight(1f)` + 中间 20dp，确认按钮用强调色文字），与 Material 的插槽式布局明显不同；⑧ 徽标有两种色调（Alert 用默认色，Accent 用 primaryContainer/primary）；⑨ `CheckboxPreference` 的勾选框可放行尾。

**对话框三种配方**（可直接照抄）：
- **A 库渲染标题**：`OverlayDialog(title = ..., summary = ..., insideMargin = DpSize(0.dp, 24.dp))` → 内容 `Column(heightIn(max = 500.dp))` + 底部等宽按钮行。
- **B 自绘居中标题**：`insideMargin = DpSize(0.dp, 0.dp)`，标题 `title4.fontSize` + Medium + 居中 + `padding(top 24, bottom 12)`，逐行 `ArrowPreference(insideMargin = PaddingValues(horizontal 24, vertical 12))`，底部通栏单个取消按钮。
- **C `WindowDialog`**：标题由库渲染，内容用自定义 `Layout`；该文件只实现"加载对话框"（`InfiniteProgressIndicator` + 返回拦截）与"确认对话框"两个 composable。

## 2.5 可复刻要点与"不要做的事"

**可直接落地（有代码依据）**：CSS 变量名沿用 §2.2 的映射；**页面左右 12dp / 卡片间距 12dp / 卡片内 16dp** 是 Miuix 观感的核心节奏（与 Material 的 16dp/13dp 明确不同）；"灰底 + 卡片"双层结构、卡片**无重阴影**（Miuix 的 FAB 显式 `shadowElevation = 0.dp`，卡片从不传 elevation，全屏唯一明显投影是悬浮底栏那条极轻阴影）；行首图标 tint = `onBackground` 且与文字间距 6dp；**三段式文字色阶**（主 `onSurface` → 次要 `onSurfaceVariantSummary` → 行尾动作 `onSurfaceVariantActions`，禁用 `disabledOnSecondaryVariant`）；设置行整行可点 + 右侧控件；分组标题在卡外、左对齐、上方 6dp；对话框按钮等宽并排；有意为之的圆角差异（徽标 6dp vs Material 4dp）；悬浮底栏全圆角胶囊；滚动收尾触感 + 关闭 overscroll 光晕；顶栏随滚动折叠；≥840dp 切侧栏；空状态是"居中一行灰字"；分隔线极细低对比（Web 对应 1px + 低透明度）；行尾图标箭头统一左间距 6dp。

**推断（结构有依据，度量需补）**：Miuix 顶栏是左对齐大标题（仓库看不到对齐方式与高度）；卡片圆角偏大（具体 dp 未找到依据）；开关样式；`title4`/`headline1`/`body1`/`body2` 的确切字号。

**明确不要做**：不要把主题预览 mock 的数值当真实度量；不要把 `0xFFDFFAE4 / 0xFF1A3825` 当全局主题色（只在非动态色板下用于状态/警示卡）；不要在 Miuix 路径用 `surfaceContainer` 当页面底色（那是 Material 侧 `Scaffold` 的 `containerColor`）。

**Material 3 一侧（供对照）**：色板是 **Monet + materialkolor 混合**（Monet 只取 primary 当种子），不是纯 dynamicColor；AMOLED 把 8 个表面色强制为黑；没有自定义 `Shapes()`（全用 M3 默认）；卡片 `shapes.large`、对话框 `shapes.extraLarge`、分段列表外 16dp 内 4dp、徽标 4dp；页面底色 `surfaceContainer`、卡片 `surfaceBright`；顶栏 `LargeFlexibleTopAppBar` + `exitUntilCollapsed`；页面留白水平 16dp / 块间距 13dp / 警示卡 h16 v12；分组标题 `titleSmall` + `primary` + `padding(start 16, bottom 8)`；分隔线 `Dp.Hairline`；动效 `MotionScheme.expressive()`；整行开关/复选/单选均带触感。Miuix 与 Material **排版体系完全不相通**（`MaterialTheme.typography` 在所有 `*Miuix*.kt` 中 0 次引用）。

---

# 三、对齐原版的重写规格

## 3.1 总策略

**不要把老项目的 Compose 布局 1:1 翻译成 React**——老项目本身就偏离原版（§3.2 列了 30 条差异）。正确做法是：

1. 以**原版**（§1）为版式唯一真源；
2. 以老项目的 `strings.xml` 为**中文文案**真源（两套文案基本同义，个别不同：原版是「WIFI模拟」，老项目是「Wi-Fi 模拟」）；
3. 以老项目的 `UiModel` / `SimulationLayout` 为**信息架构**真源（哪个页面有哪些字段、哪些动作）；
4. 以当前 `webui/src/*.ts(x)` 为**能力**真源（哪些后台命令已接通）。

## 3.2 老项目与原版的差异（★ = 必须补的）

**骨架**：① ★ 原版抽屉是 1 个单选分组 + **3 个可折叠子菜单**（View / Settings / More），老项目是扁平 7 项 + 2 个静态分组标题，没有折叠；② ★ `More` 组 12 项老项目完全没有（功能上可能不需要，但"折叠组"这个形态是版式特征）；③ ★ 原版抽屉头有背景图 + 图标 + 应用名 + PRO 行 + 白底绿字按钮（minHeight 220dp），老项目是纯文字品牌块；④ ★ 原版 `AppBarLayout height=300dp` + `CollapsingToolbarLayout` 可折叠，老项目是固定 260dp 主色块 + 自绘 56dp 行 + 112dp 标题行，**不折叠**；⑤ ★ 原版底部居中固定免责声明，老项目没有；⑥ 抽屉图标 tint 两边等价；⑦ ★ 原版 FAB 锚在内容容器上、随滚动隐现（`a3x` = end 侧，即靠右；**纵向位置未确定**），老项目固定在右上角且不随滚动。

**主页面版式**：⑧ ★ 原版区块头 = 灰 14dp 标题 + 弹簧 + **32dp 箭头**（语义"查看全部"），老项目是标题 + 搜索图标按钮；⑨ ★ 原版空态 = **80dp 插画** +「点击右上角 "+" 按钮进行添加」，老项目是 36dp 图标 + 圆形灰底；⑩ ★ 原版列表行 = **40×40 绿色圆底** + 图标 + 名称(bold 16dp) +「经纬度:」+ 值，老项目是 Card + ListItem；⑪ 原版列表卡是白底 4dp 圆角、左右 8dp、下 24dp 的**包裹式卡片**，老项目每项是独立 Card；⑫ ★ 原版四个主页面**完全同构**（共用同一骨架），老项目也同构但各页目标卡内部差异大、没有统一操作行。

**控件形态**：⑬ ★ **绿色药丸主按钮**（#008577 / 高 28dp / 圆角 14dp / 白字 14sp / 禁用 #a4008577 / 内嵌 #f7cd1a 转圈）——**最关键的一条**，老项目用 M3 `Button`（高 40dp、全圆角、无内嵌转圈）；⑭ ★ **240dp 固定宽锚定小面板**（基站 `c7.xml`、速频 `c9.xml`），老项目是 280dp 下拉菜单和全宽对话框；⑮ ★ 原版开关行**不整行可点**，老项目的 `ToggleRow`/`SettingSwitch`/`IconSwitch` 全部整行可点；⑯ ★ 原版分段选择器是白药丸 + 绿字（高 30dp），老项目退化成按钮 + 下拉菜单；⑰ ★ 原版地图页右侧**竖排 FAB 组**（4/5 个），老项目没有；⑱ ★ 原版路线页地图顶部有**绿色搜索卡**（路线图标 + 起终点连线 + 搜索起点/终点 18dp 白字 + 3 个出行方式按钮），老项目没有；⑲ 原版独立模拟页主按钮是「打开独立模拟」+ 多应用角标 "+1"，老项目改成开关 + 应用勾选列表（语义不同：老项目讲"生效范围"，原版讲"仅对一个应用生效的模式"）；⑳ 原版位置卡有 `"NONE • NONE"` 省市/地址摘要副值行，老项目缺。

**设置**：㉑ ★ 原版设置是**单个 PreferenceScreen**、7 个分类的长列表，老项目是"索引页 + 8 个子页"；㉒ ★ 原版有而老项目没有的设置项约 11 个（模拟GPS信号、允许搜索GPS信号、模拟SIM卡信息、X86平台组件、系统就绪时长、SELinux 与 SELinux自动恢复、Xposed兼容、日志记录、清理运行环境、用户协议、隐私政策、联系开发者）；㉓ ★ 原版「运行模式」（NOROOT / ROOT / 切换模式）是抽屉 Settings 组第一项，老项目用"允许的授权方式多选"替代。

**老项目有、原版没有的（建议保留）**：地图 Key 配置与 Photon/Valhalla 端点配置、每地点基站列表与基站/Wi-Fi/GPS 实时采集、备份导出导入与分享码、授权方式多选（Zygisk / LSPosed / Root / Shizuku / Mock Location）、卫星与计步页、摇杆有真实开关状态。**建议删掉**：`ModalBottomSheet` 的「保存的位置」（死代码，且原版全仓无 BottomSheet——**重写时不要引入底部抽屉式弹层**）。

## 3.3 实施清单（按优先级）

**P0 版式骨架**（做完这几项，界面"看起来就像 Fake Location 了"）：

| 序 | 任务 | 状态 |
|---|---|---|
| 0.1 | 重写 CSS 令牌层（主色 #008577、页面底 #fafafa、卡片 #ffffff、正文 #4e6161 等） | **已完成** |
| 0.2 | 三段式外壳（目标卡 → 区块头 → 列表卡） | **已完成**（固定页头 + 独立滚动区 + 页尾免责声明） |
| 0.3 | `TargetCard` + 绿色药丸（28dp 高 / 14dp 圆角 / 内嵌黄色 loading） | **已完成**（目标卡重排 + `pill`） |
| 0.4 | `SectionHeader`（灰标题 + 「查看全部」32dp 箭头） | **已完成**（`webui/src/SectionHeader.tsx`；位置页已用，路线页与 Wi-Fi 页仍待替换） |
| 0.5 | `RowCard`（40dp 绿色圆底图标 + 名称 bold + 坐标） | **已完成**（`webui/src/RowCard.tsx`） |
| 0.6 | `EmptyState`（80dp 插画 + 提示） | **已完成**（`webui/src/EmptyBox.tsx`；原版位图改画成内联 SVG，仍随主题色走） |
| 0.7 | FAB 靠 **end** 侧 | **已完成，位置取右下角**（见下方说明） |
| 0.8 | 底部免责声明 | **已完成**（放在内容末尾，不是固定悬浮） |

**FAB 的位置为什么是右下而不是右上**：原版的 FAB 锚在内容容器上（`app:vt=@id/iz`），只有 `a3x` = end 侧有资源依据，**纵向位置没有依据**。先按"右上角"实现时，它正好压住页面顶部提示条的关闭按钮，两个浏览器用例因此点不中目标而超时——这不是测试挑剔，是真实的遮挡。改为右下角后既避开遮挡，也与空态文案「点击右下角 "+" 按钮进行添加」自洽（原版写的是"右上角"，因为它把 FAB 放在右上）。

**位置页现在的三段结构与两处有意的差异**：目标卡 → 区块头（历史位置 + 计数 + 「查看全部」箭头）→ 列表卡或空态，与 `ci.xml` 一致。差异是：① 原版那一屏**只有**这三段，我们额外把「模拟功能」（作用范围 / 基站模拟 / 摇杆）与「卫星」（GNSS / NMEA）两张卡片留在同一页——这是本项目自己的功能，原版没有对应页面，留在首页是为了不把常用开关藏进二级页；② 历史位置行右侧比原版多出置顶 / 编辑 / 删除三个按钮（原版只有一个默认隐藏的图标位），因为这三个操作在原版里没有对应物。

**P1 目标卡的功能入口**（还原度提升最大的一段）：`AnchorPanel`（固定 240px 宽）分别承载「基站设置」与「速/频」（运动速度 + SeekBar + 5 个预设 + 速度浮动 + 步频模拟 + 步频输入）；目标卡右侧功能入口改成原版形态（「基站」文字 + 30dp 圆形图标按钮、「摇杆」文字 + Switch）；补副值行（省/市或地址摘要占位，**无数据时显示 `NONE • NONE` 风格占位即可**）；路线页目标卡补「起点/终点」可视化列与「速/频」「循环」。

**P2 抽屉与二级页**：抽屉改为「4 主项 + View 折叠组(3) + Settings 折叠组(2) + More 折叠组」；抽屉头改成"图标 + 应用名 + 副标题 + 一个圆角按钮"（原版是 PRO/解锁，本项目可放"运行模式"入口）；分段选择器改成白药丸 + 绿字；地图页加右侧竖排 FAB 组 + 顶部绿色搜索卡；路线编辑器独立成屏（现在是同屏两卡）。

**P3 设置与长尾**：设置改回"分类长列表"观感（位置模拟 / 基站模拟 / 步频模拟 / 其它 / 关于）；开关行去掉整行可点 + 行间 1dp 分隔线 `#f2d4d4d4`；列表行右侧箭头**原版未找到依据，建议不加**；观测三页（GPS / 基站 / Wi-Fi）补齐，用同一个只读页组件。

## 3.4 现有 React 代码复用判定

- **全量复用、不要动**：`control.ts` / `cells.ts` / `joystick.ts` / `coordinates.ts` / `routeDraft.ts` / `routeImport.ts` / `backup.ts` / `fileExport.ts`（纯逻辑，与 UI 无关，单测已覆盖）。
- **复用**：`AppPicker.tsx`（搜索/系统应用过滤/刷新完整）、`RouteLibrary.tsx`（保存/使用/导出 GPX/删除完整）、`BackupPanel.tsx`。
- **复用 + 加壳**：`MapPicker.tsx`（地图与折线可用，需补 FAB 组与顶部绿色搜索卡）、`CellPanel.tsx` / `TelephonySettings.tsx`（保留查询与应用数据逻辑，外壳从"全屏对话页"改为原版的「附近基站」列表 + 240px 锚定面板入口）、`WifiPanel.tsx`（逻辑保留，外观重写为「目标 Wi-Fi 卡 + 历史 Wi-Fi 列表」）、`PositionEditor.tsx` / `ImportPlaceSheet.tsx`（坐标输入与校验保留，改字段文案）。
- **要改**：`Controls.tsx` 的 `SwitchRow`（去整行可点 + 加分隔线）、`Segmented`（配色改成白药丸 + 绿字）。
- **推倒**：`FeatureMenus.tsx`（当前是"3 个图标按钮 + 弹层"，原版是"目标卡右侧操作行 + 240dp 锚定面板"）、`App.tsx` 的 JSX 外壳（离原版最远的一段：无主色头、无三段式、无绿色药丸、FAB 在标题行）。
- **保留机制、替换内容**：`style.css` 的三层结构（令牌/组件/布局）设计良好，把第一层值换成原版色值、组件层换成原版形态即可。
- **建议收敛**：`theme.ts` 的 material/miuix 双外观——原版只有一套浅色 Material 主题，保留双外观会让"还原原版"难以验证；可保留开关但把 `material` 设为默认并对齐原版色值。

## 3.5 页面清单（对齐原版抽屉 4 + View 3 + Settings 2，More 折叠）

| 目的地 | 抽屉位置 | 页面组件 | 依赖数据 |
|---|---|---|---|
| 位置模拟 | 主项 1 | `SimulationPage` + 历史位置 | `status.config.position`、`places`(localStorage)、`telephony.cells_enabled` |
| 路线模拟 | 主项 2 | `SimulationPage` + 历史路线 | `state.route`、`routes`(localStorage)、`repeat_count` |
| Wi-Fi 模拟 | 主项 3 | `SimulationPage` + 历史 Wi-Fi | `state.wifi`（`set_wifi` 已接通，**输出通道仍未实现**） |
| 独立模拟 | 主项 4 | `ScopeScreen` | `scopeDraft`(localStorage)、`kernelsu.getPackagesInfo` |
| GPS 位置 | View 1 | 观测页（只读） | `state.location_hook_ready`、`state.config.position` |
| 附近基站 | View 2 | `CellPanel`（已有） | `justlocationd cells`、`state.telephony` |
| 附近 Wi-Fi | View 3 | 观测页（只读） | **需要新的后台能力（附近扫描结果）** |
| 运行模式 | Settings 1 | 授权与运行 | `state.hook_connected`、`installed` |
| 设置 | Settings 2 | 设置分类列表 | `state`、`localStorage` |
| （更多） | More（折叠） | 关于 / 备份 / 诊断 | 已有 `BackupPanel` |

建议的组件目录：`webui/src/ui/` 下放 `tokens.css`、`AppShell.tsx`（+ `Drawer.tsx` / `DrawerSubMenu.tsx`）、`SimulationPage.tsx`、`TargetCard.tsx`、`TargetActions.tsx`、`PillButton.tsx`、`SectionHeader.tsx`、`EmptyState.tsx`、`RowCard.tsx`、`AnchorPanel.tsx`、`Segmented.tsx`、`FabStack.tsx`、`MapScreen.tsx`、`RouteEditorScreen.tsx`、`ScopeScreen.tsx`、`SettingsPage/`、`Disclaimer.tsx`。

## 3.6 硬约束（改前端时必须保留，否则测试变红）

- **可访问名称精确匹配**（`getByRole('button', {name, exact:true})`）：`打开导航`、`关闭导航`、`刷新后台状态`、`关闭提示`、`添加位置`、`开始模拟`、`停止模拟`、`处理中…`、`独立模拟菜单`、`基站菜单`、`摇杆菜单`、`作用范围`、`完成`、`返回`、`启用独立模拟`/`禁用独立模拟`、`基站与运营商`、`打开摇杆`/`关闭摇杆`、`最高速度（km/h）`、`保存位置`、`地图选点`、`使用此位置`、`完成路线`、`添加到路线`、`撤销最后一个点`、`导入 GPX`、`速度（km/h）`、`播放次数`、`每次间隔（秒）`、`开始路线`/`暂停路线`/`继续路线`/`停止路线`、`保存路线`、`搜索历史位置`、`搜索应用`、`刷新列表`、`跟随系统`/`浅色`/`深色`、`返回位置模拟`、`查询附近基站`、`联网刷新`、`应用这份基站数据`、`保存模拟设置`、`模拟基站`、`模拟 SIM 运营商`、`仅使用离线数据`、`导入离线数据`、`导出当前数据` 等。
- **类名 / 结构**：`.target-actions > .primary`（必须是**直接子元素**）、`.primary.stop`、`.feature-menus > .selected`、`.scope-topbar`、`.app-list`、`.cell-panel`、`.map-screen`、`.target-card`、`.history`。
- **role 契约**：错误必须是 `role="alert"`（含路线校验错误）；提示用 `role="status"`，**同一时刻页面上只能有一个可见 status**；`dialog` 的 `name` 必须是 `独立模拟`/`基站`/`摇杆`/`地图选点`/`地图规划`/`附近基站`。
- **布局断言**：所有断点下 `document.documentElement.scrollWidth <= innerWidth`；320px 宽下弹层不溢出；作用范围页与基站页 `scrollHeight <= innerHeight`。
- **存储形状**：`justlocation.scope` = `{mode, packages}`；`justlocation.places` 数组项含 `position.altitude`；`justlocation.route` 的 points 长度与草稿内容；`justlocation.theme`、`justlocation.joystick.speed`。
- **命令格式**：摇杆命令必须仍含 `--es speed 7.2 --ez open true`；请求帧仍是单参数 base64 JSON；瓦片请求仍走 `tile.openstreetmap.org`。
- 若重写导致上述任一断言失败，**优先改测试的定位方式或同步更新测试**（测试是旧 UI 的产物），但必须在提交说明里显式写出改了哪些断言，不要静默绕过。

## 3.7 改版前的 React 结构（只保留仍有用的部分）

这些是 2026-09-11 重做之前的现状速查，**多数已被本轮提交取代**，保留的是仍会影响改动的接口与契约：

- 前端与后台的通道：`createClient(exec)` 把 `{version:1, ...command}` → UTF-8 → `btoa` → 作为**单个 shell 参数**执行 `justlocationd request <b64>`；响应校验 `errno !== 0` 抛 `stderr`，再校验 `version===1 && typeof ok==='boolean' && 'config' in state`；成功只返回 `state`。无 `window.ksu` 时抛「请从 KernelSU 管理器打开模块面板」（e2e 有断言）。摇杆、导出、基站走各自的分支（`joystick.ts` 的 `pm path`/`am start`/`am stopservice`、`fileExport.ts` 的 base64 落盘、`cells.ts` 的 `justlocationd cells <b64>`）。
- **前端不发送心跳**：后台按 Java/Zygisk 侧 `hook_status`、`telephony_hook_status` 的到达时间判定（窗口 3 秒）→ `hook_connected` / `phone_connected`；`location_hook_ready = hook_connected && installed`，`cell_hook_ready` 还要 `cells_installed` 与 `cell_callbacks_installed`。
- 后台协议：请求 `{version:1, op, ...}`，响应 `{version:1, ok, error, state, cells}`；命令枚举带 `deny_unknown_fields`（**多塞字段会被拒绝**），`TelephonyConfig` / `Subscription` 同样严格；帧上限 64 KiB（前端也在 65536 处预拦）。前端已使用 `status` / `start` / `stop` / `update` / `start_route` / `pause_route` / `resume_route` / `set_telephony` / `set_cell_region`；**`drive`、`shutdown`、`query_cells` 后台有实现但前端未接入**（`hook_status` 与 `telephony_hook_status` 由 Java 侧发送）。配置持久化为 `version:3` 单文件、原子替换、写失败回滚。
- 前端类型与工具：`Position`（latitude/longitude/altitude/accuracy/speed/bearing）、`Scope`（`{mode:'all'}` 或 `{mode:'apps',packages}`）、`RoutePlan`（points/speed/repeat_count/repeat_delay，速度单位 m/s）、`RouteState`、`Cell*` 系列；`parsePosition` 负责空值/非有限数/越界校验并补默认 `accuracy:5, speed:0, bearing:0`。
- CSS 结构（`style.css`）：三层（令牌 / 组件 / 布局），全屏页的滚动契约是"外层 `height:100dvh; overflow:hidden` + 内层 flex 滚动"。
- 测试基建：`App.test.tsx` 注入 `client`/`loadApps`/`joystick`；e2e 用 `window.ksu.exec` 替身 + `atob` 解帧。**新 UI 保持同一注入点即可复用现有夹具。**

## 3.8 当前实现与验证状态

**与原版的有意分歧**：原版把"独立模拟、基站、摇杆"放在目标卡的图标按钮里、点开是弹层；本项目改成整行开关（并给卫星通道也加了开关），因为开关的语义层就是可见的 `input[type=checkbox]`，勾选、禁用、键盘空格与指针点击都由浏览器处理——曾经用"隐藏 1px 输入框 + label 转发点击"的写法，真机上出现过点不动与一次点击切换两次，已废弃。**注意**：`f47f57c` 的提交信息误称这次改动把 GNSS 与 NMEA 做成了整行开关，实际没有；这两个开关的去留仍未定。

**当前面板结构**（2026-09-11 的状态，正在按 §3.3 的 P0 改写）：

- 侧边导航四项：位置模拟、路线模拟、Wi-Fi 模拟、设置；窄屏变抽屉，另有全屏子页（作用范围、基站、地图、导入）。
- 位置页自上而下：目标位置卡片与启停按钮 →「模拟功能」卡片（作用范围、基站模拟、摇杆三个整行开关，各自下方就地展开次要操作）→「卫星」卡片（GNSS 状态、NMEA 报文）→ 历史位置列表（搜索、置顶、编辑、删除）。
- 作用范围：开关表示"是否只对指定应用生效"，范围页只负责勾选应用，不再提供模式单选；关闭再打开保留已选应用，未选应用时拒绝开启并说明原因。
- 路线页分两页：默认「路线管理」（新建路线 / 继续编辑草稿 / 已保存路线列表），新建或编辑某条路线才进入「编辑路线点」页。
- 基站页按卡片组织：目标位置 / 总开关 / 运营商与 SIM / 查询 / 数据。总开关直接写后台配置，查询结果需点「应用这份基站数据」才用于模拟。
- 地图：图源可在 OpenStreetMap、OpenFreeMap、高德、腾讯、百度、自定义地址间切换，支持搜索、定位到当前位置、直接输入经纬度；按图源坐标系换算，内部始终 WGS84。
- 导入位置：粘贴地图链接或经纬度，识别后显示来源与坐标，可修正坐标系再存入历史位置；只进历史列表，不改动正在运行的模拟。
- 外观：Material 3 与米 UI（Miuix 风格）两套，各自跟随系统/浅色/深色，写到 `<html data-style>` 与 `data-theme`。

**验证口径**：前端单元测试与 15 条浏览器用例、后端测试、类型检查与生产构建均通过（具体条数随提交变化，见备忘录）。浏览器里的 KernelSU 桥是**测试替身，不能替代管理器和 Android 服务的真机验收**；本轮界面改动尚未部署到手机。
