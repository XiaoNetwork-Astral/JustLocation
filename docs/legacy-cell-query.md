# 目标位置附近基站：源码补充分析

分析日期：2026-09-10；样本 FakeLocation 1.5.2。以下结论来自现有运行时 DEX 的 Java / smali 与资源，未调用旧应用服务，也未连接手机。

## 结论与调用链

**旧应用支持人在其他地方时查询目标附近基站：它发送目标经纬度到自己的远程服务。** 此功能与 `C3735 → C3377` 采集手机当前基站是两条链。旧报告 7.2 中“不会从目标经纬度自动查出一组真实基站”仅适用于 Hook 输出端的缺数据分支，不能概括整个应用。

`C3818.C3820` 开启基站 → 目标没有 `nearbyCells` → `C3818.m11244(target.latitude, target.longitude)` → 创建 `i90` → `C2776.m9061` 补请求元数据 → `InterfaceC1660.m6908` 的 `@POST("cell/queryNearby")` → `yl0<List<C2646>>` → `C3191.C3237.mo10003` → `C0887.m5221`按目标 ID 保存 `nearbyCells`。

关键点：该查询函数没有读取手机实时位置，没有先扫描本地基站，没有传入搜索半径、分页或运营商筛选。经纬度直接取位置记录；请求体没有坐标系字段，此处没有转换。服务端期待的坐标系仍需真实请求及结果对照确认。

## 请求地址与字段

`j01` 的 StringFog 解码经 `tg1 → bh1`，算法是循环 XOR 后 UTF-8。构造 `C2776` 时使用：

- 主地址：`https://api.fakeloc.cc:4430/FakeLocation/`
- 备用地址：`https://fakelocation.api.lerist.dev:4430/FakeLocation/`
- 完整主查询地址：`https://api.fakeloc.cc:4430/FakeLocation/cell/queryNearby`

`j01` 另有不带端口的地址常量，但本构造链没有使用它。网络回退代码替换 scheme/host/port，保留原路径；这些是样本内常量，不代表当前服务在线或接口仍可使用。

业务请求模型 `i90` 只有 `latitude: double`、`longitude: double`，继承 `C1959`。`C2776.m9061` 还填充时间戳、应用版本/包名/签名摘要、渠道、时区/语言/国家、设备 ID/型号/SDK、用户 ID/token（存在登录信息时）、identity 和 rooted 状态。`signature` 来自应用签名字符表示的 MD5，不是本处计算的经纬度请求签名。没有发现该链内额外的经纬度加密或签名；不能据此排除未确认的转换器/服务端校验。HTTP 编码、Content-Type 与实际省略字段留待捕获请求确认。

开关启用路径调用 `x92.m4874()` 作资格判断，不满足时取消开关并提示。客户端资格检查与服务端鉴权是两件事；现有源码不足以证明接口匿名可用。不将旧应用 token 或身份字段硬编码到 JustLocation。

## 回包、保存和刷新

`yl0` 包含 `code`、`message`、`body`、`extras`、`remark`、`returnTime`；业务成功判断是 `code == 200`，与 HTTP 成功检查分开。

`body` 元素 `C2646` 的字段如下。这里列的是反编译模型，未把它伪装成实际抓到的响应样本。

| 字段 | 类型 | 已确认用途 / 未确认边界 |
| --- | --- | --- |
| radio_type | string | 模型常量 GSM/CDMA/LTE/UMTS/WCDM；未发现 NR 常量 |
| mcc、mnc | int | 移动国家码、网络码；int 不能保留 MNC 前导零位数 |
| lac | int | 模型统一区域字段；具体制式映射见 Hook 构造器 |
| cellid | long | 小区身份；不是始终安全的 32 位整数 |
| unit | int | 存在该字段；具体各制式语义不能仅凭名称认定 |
| lat、lon | double | 基站位置，供距离/信号估算使用 |
| range | float | 供覆盖范围/信号估算使用；真实数据单位及缺失值待样本核对 |

非 null 列表通过目标 ID 写回位置记录，并刷新当前选择和列表。开启时已有列表会复用；刷新按钮重新查询。目标来源为采集或导入时，资源 `kk` 明确提示重新查询会覆盖已采集基站，要求确认；空结果、查询异常与查询中分别有独立文案。

旧实现存在需要避开的状态问题：成功回调即使收到空列表，后半段仍调用 `mo10002(true)`；回调使用可变的 `f14322`，未捕获发起请求的目标 ID 或请求版本。查询中切换目标可能把旧结果关联到新目标。这是根据回调读写推导的风险，尚未在旧 App 复现。新实现应绑定目标和请求版本，迟到结果不能改变当前场景；失败保留旧数据，空结果明确展示。

## 数据库来源与可替代实现

当前 Java 源码仅找到上述 `cell/queryNearby` 接口，没有查到该链直接调用 OpenCellID 等第三方服务；尚未找到服务端源码或随包分发的区域基站库。**无法从客户端确定服务端使用哪家数据库、搜索半径、去重规则、更新频率、排序或覆盖率。** 字段相似不足以证明它来自 OpenCellID。

独立实现可以使用区域数据文件，或让用户配置自己可用的区域查询服务。OpenCellID 有按经纬度边界框查询的接口，需要用户 API key；它查询数据库，与设备是否身处目标无关。下载也需要 token，现阶段没有取得区域数据文件。[区域查询文档](https://docs.opencellid.org/docs/api/cells-in-area)、[下载入口](https://opencellid.org/downloads.php)。如采用该数据源，需保留其署名及许可信息，覆盖稀疏不能等同于当地没有信号。[署名说明](https://docs.opencellid.org/docs/attribution)。这些是候选实现资料，不是旧样本数据来源的证据。

JustLocation 的数据源、缓存和 Android 输出应分开：查询获得真实小区身份与位置；场景选择和信号估算使用当前目标；Hook 负责按作用范围交付同一场景。不能根据经纬度凭空编造“真实附近基站”。已有数据在离线时可复用，但必须保留查询中心、来源和时间，移动到其他地区后不能继续标称为当地数据。

## 已保存资源与后续配合清单

工作区资源：`build/environment-reference/legacy-cell-query/`。包含入口、接口、请求/响应、网络配置、基站模型、位置保存的源码副本，端点解码结果，以及 OpenCellID 区域查询、数据格式、访问限制、署名文档网页副本；清单见 `manifest.json`。副本只包含静态源码和公开文档，没有提取运行时账号凭据。它们用于分析，不编入模块。

需要手机后集中处理（当前不需要用户操作）：

1. 在旧 App 正常授权会话中，分别查询两个相距较远的目标，保存脱敏请求结构及成功响应；删除 token、用户 ID、设备 ID 等值。比较请求坐标与选点坐标、返回中心、制式/单位、空字段与数量。
2. 保存一次空结果或自然失败，以及查询后离线重开的位置记录；确认缓存、覆盖与错误展示，不反复请求消耗额度。
3. 若选择 OpenCellID，用户自行取得自己的访问资格/token；凭据留在本地配置，不粘贴聊天。下载所需地区数据或完成最小查询后再判断覆盖是否适用。
4. 新模块电话通道接入后，按实际卡槽/订阅验证同步、异步和监听输出；检查目标应用、普通应用及停止模拟三种状态。此项验证不能由无 Zygisk 的 AVD 替代。

## 证据定位

原始 Java 目录：`D:/temp/2026.9.1-FakeLocation/.codex/analysis/runtime-jadx-main/sources/androidx/appcompat/view/widget/`。

- `C3818.java`：`m11244` 查询、`C3820` 启用、`C3821/C3822` 回包、`ViewOnClickListenerC3824` 刷新。
- `InterfaceC1660.java` 与 `analysis/runtime-smali-main/smali/androidx/appcompat/view/widget/υ.smali`：POST 路径、Body 注解及泛型返回类型相互核对。
- `i90.java`、`C1959.java`、`C2776.java`：参数及公共元数据；`j01.java`、`tg1.java`、`bh1.java`：地址与解码。
- `C1920.m7348`、`C4373.m12373`：签名摘要来源。
- `yl0.java`、`C2646.java`：响应及小区模型。
- `C3191.C3237.mo10003`、`C0887.m5221`、`C4077.java`：绑定位置并保存。
- `analysis/jadx/resources/res/values/public.xml`、`strings.xml`：资源 2131952033–2131952038（ki–kn）。
