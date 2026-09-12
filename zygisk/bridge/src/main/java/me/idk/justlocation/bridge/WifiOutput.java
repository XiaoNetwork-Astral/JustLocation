package me.idk.justlocation.bridge;

import android.net.wifi.ScanResult;
import android.net.wifi.SupplicantState;
import android.net.wifi.WifiInfo;
import android.net.wifi.WifiSsid;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;

/**
 * 合成 Wi-Fi 读数：连接信息与扫描列表。
 *
 * <p>面板保存的目标列表就是输出内容：**第一条作为当前连接的网络**，其余作为附近网络，
 * 与原版"首项作为连接网络，其余作为附近网络"的模型一致。作用范围与新鲜度沿用
 * {@link SessionSnapshot}——和定位、卫星两条通道同一套规则；后台心跳过期或本应用不在
 * 范围内时，调用方一律拿到系统数据。
 *
 * <p>信号字段直接取配置值（原版模型缺省 `rssi = 200`、`linkspeed = 866`、
 * `frequency = 5745`）。200 不是常规 dBm 读数，这里也不据此推断信号强弱，只是照搬模型。
 *
 * <p><b>为什么用反射</b>：`WifiInfo` / `ScanResult` / `WifiSsid` 的构造与赋值入口是
 * `@hide`，编译期不可见但运行时存在（Android 15 上已核对签名，见
 * `build/wifi-reference/`）。这里在类初始化时一次性解析这些方法并缓存，失败就整体不接管，
 * 绝不因为某个 setter 缺失而返回半成品对象。
 */
final class WifiOutput {
    /** 合成能力字段：原版也没有真实能力位，这里用一个中性的通用写法。 */
    private static final String CAPABILITIES = "[ESS]";
    /** 一个偏高的 5GHz 常用频段带宽，仅用于让字段看起来合理，不代表真实测量。 */
    private static final int CHANNEL_WIDTH_MHZ = 80;

    /**
     * 合成连接的"网络号"与"评分"。
     *
     * <p>取值照搬原版模型（`setNetworkId(1000)`、`score = 60`）：系统给真实连接的评分就是这个量级，
     * 应用常拿它判断"是不是连着网"。给 {@code -1}（`INVALID_NETWORK_ID`）会让合成对象看起来没连上。
     */
    private static final int NETWORK_ID = 1000;
    private static final int SCORE = 60;

    private static final java.lang.reflect.Constructor<WifiInfo> NEW_INFO;      // WifiInfo()
    private static final java.lang.reflect.Constructor<ScanResult> NEW_SCAN;    // ScanResult(WifiSsid,String,String,int,int,long,int,int)
    private static final Method SET_SSID;         // WifiInfo.setSSID(WifiSsid)
    private static final Method SET_BSSID;        // WifiInfo.setBSSID(String)
    private static final Method SET_MAC;          // WifiInfo.setMacAddress(String)
    private static final Method SET_RSSI;         // WifiInfo.setRssi(int)
    private static final Method SET_LINK_SPEED;   // WifiInfo.setLinkSpeed(int)
    private static final Method SET_FREQUENCY;    // WifiInfo.setFrequency(int)
    private static final Method SET_NETWORK_ID;   // WifiInfo.setNetworkId(int)
    private static final Method SET_STATE;        // WifiInfo.setSupplicantState(SupplicantState)
    private static final java.lang.reflect.Field SCORE_FIELD; // WifiInfo.score
    private static final Method FROM_UTF8;        // WifiSsid.fromUtf8Text(CharSequence)
    private static final Method SCAN_SSID;        // ScanResult.setWifiSsid(WifiSsid)
    private static final Method SCAN_STANDARD;    // ScanResult.setWifiStandard(int)
    private static final Method SCAN_CHANNEL_WIDTH; // ScanResult.setChannelWidth(int)

    static {
        java.lang.reflect.Constructor<WifiInfo> newInfo = null;
        java.lang.reflect.Constructor<ScanResult> newScan = null;
        Method setSsid = null, setBssid = null, setMac = null, setRssi = null, setLinkSpeed = null, setFrequency = null;
        Method setNetworkId = null, setState = null;
        java.lang.reflect.Field scoreField = null;
        Method fromUtf8 = null, scanSsid = null, scanStandard = null, scanWidth = null;
        try {
            // 必需项：没有这些就构造不出可用的对象，整条通道直接关闭。
            newInfo = WifiInfo.class.getDeclaredConstructor();
            newInfo.setAccessible(true);
            newScan = ScanResult.class.getDeclaredConstructor(WifiSsid.class, String.class, String.class,
                    int.class, int.class, long.class, int.class, int.class);
            newScan.setAccessible(true);
            setSsid = WifiInfo.class.getMethod("setSSID", WifiSsid.class);
            fromUtf8 = WifiSsid.class.getMethod("fromUtf8Text", CharSequence.class);
            scanSsid = ScanResult.class.getMethod("setWifiSsid", WifiSsid.class);
        } catch (Throwable error) {
            android.util.Log.w("JustLocation", "Wi-Fi object APIs unavailable", error);
        }
        // 可选：不同 ROM 暴露的字段并不一致（例如 HyperOS 的 ScanResult 就没有
        // setLinkSpeed / setChannelWidth）。少一个只影响那个字段，不该让整条通道关闭。
        try { setBssid = WifiInfo.class.getMethod("setBSSID", String.class); } catch (Throwable ignored) { }
        try { setMac = WifiInfo.class.getMethod("setMacAddress", String.class); } catch (Throwable ignored) { }
        try { setRssi = WifiInfo.class.getMethod("setRssi", int.class); } catch (Throwable ignored) { }
        try { setLinkSpeed = WifiInfo.class.getMethod("setLinkSpeed", int.class); } catch (Throwable ignored) { }
        try { setFrequency = WifiInfo.class.getMethod("setFrequency", int.class); } catch (Throwable ignored) { }
        try { setNetworkId = WifiInfo.class.getMethod("setNetworkId", int.class); } catch (Throwable ignored) { }
        try {
            setState = WifiInfo.class.getMethod("setSupplicantState", SupplicantState.class);
        } catch (Throwable ignored) { }
        try {
            scoreField = WifiInfo.class.getDeclaredField("score");
            scoreField.setAccessible(true);
        } catch (Throwable ignored) { }
        try { scanStandard = ScanResult.class.getMethod("setWifiStandard", int.class); } catch (Throwable ignored) { }
        try { scanWidth = ScanResult.class.getMethod("setChannelWidth", int.class); } catch (Throwable ignored) { }
        NEW_INFO = newInfo; NEW_SCAN = newScan;
        SET_SSID = setSsid; SET_BSSID = setBssid; SET_MAC = setMac; SET_RSSI = setRssi;
        SET_LINK_SPEED = setLinkSpeed; SET_FREQUENCY = setFrequency;
        SET_NETWORK_ID = setNetworkId; SET_STATE = setState; SCORE_FIELD = scoreField;
        FROM_UTF8 = fromUtf8; SCAN_SSID = scanSsid; SCAN_STANDARD = scanStandard;
        SCAN_CHANNEL_WIDTH = scanWidth;
    }

    private final SessionSnapshot scope;
    private final boolean enabled;
    private final List<WifiSettings.Target> targets;

    private WifiOutput(SessionSnapshot scope, boolean enabled, List<WifiSettings.Target> targets) {
        this.scope = scope;
        this.enabled = enabled;
        this.targets = List.copyOf(targets);
    }

    /** 所有必需的隐藏入口是否都在。缺任何一项就整条通道不接管。 */
    static boolean usable() {
        return NEW_INFO != null && NEW_SCAN != null && SET_SSID != null && FROM_UTF8 != null && SCAN_SSID != null;
    }

    /** 某个可选 setter 是否可用；不可用就跳过那个字段，而不是让整条通道关闭。 */
    private static boolean has(Method method) {
        return method != null;
    }

    /**
     * 心跳解析：先做纯解析（见 {@link WifiSettings}），再要求运行时的隐藏接口都在。
     * 任一隐藏入口缺失就整条通道不接管，避免输出半成品对象。
     */
    static WifiOutput parse(String response, long nowMs) throws Exception {
        if (!usable()) return null;
        WifiSettings settings = WifiSettings.parse(WifiSettings.fromJson(response), nowMs);
        if (settings == null) return null;
        return new WifiOutput(settings.scope(), true, settings.targets());
    }

    /** 本应用当前是否由模拟接管。 */
    boolean appliesTo(String packageName, long nowMs) {
        return enabled && !targets.isEmpty() && scope.appliesTo(packageName, nowMs);
    }

    /** 当前会输出的网络名称，供日志与诊断使用。 */
    String connectedSsid() { return targets.get(0).ssid(); }

    /**
     * 当前连接的合成读数。
     *
     * <p><b>这里绝不能做脱敏</b>（2026-09-12 真机踩到的坑，代价是一轮重启）：
     * `WifiInfo.getApplicableRedactions()` 返回的是**这个类支持哪几种脱敏**（三个位全开），
     * 不是"当前调用方需要脱敏哪几位"。照它调 `makeCopy(...)` 等于把 SSID 与 BSSID 全抹掉，
     * 应用侧读到的就是 `<unknown ssid>` 配 `02:00:00:00:00:00`——看上去像"挂钩没生效"，
     * 其实是我们自己把内容擦了（当时 `rssi` 仍是配置值 `-42`，正是这一点暴露了真相）。
     * 平台的服务端实现是**先按调用方权限算出该脱敏的位**再传进来；我们不去复刻那套权限推导，
     * 因为作用范围已经限定了只有被选中的应用会走到这里，其余调用方拿到的仍是系统原值。
     * 原版同样直接返回合成对象，不套任何脱敏位。
     *
     * <p>字段集合对齐原版模型（`WifiInfo` 的 `setSSID/setBSSID/setMacAddress/setRssi/
     * setLinkSpeed/setFrequency/setNetworkId/score/setSupplicantState`）：只填前几个的话，
     * 应用会读到"名称对得上、但状态是未连接"，照样判定没在 Wi-Fi 上。
     */
    WifiInfo connectionInfo() throws Exception {
        WifiSettings.Target target = targets.get(0);
        WifiInfo info = NEW_INFO.newInstance();
        SET_SSID.invoke(info, ssid(target.ssid()));
        if (!target.bssid().isEmpty()) {
            if (has(SET_BSSID)) SET_BSSID.invoke(info, target.bssid());
            if (has(SET_MAC)) SET_MAC.invoke(info, target.bssid());
        }
        if (has(SET_RSSI)) SET_RSSI.invoke(info, target.rssi());
        if (has(SET_LINK_SPEED)) SET_LINK_SPEED.invoke(info, target.linkSpeed());
        if (has(SET_FREQUENCY)) SET_FREQUENCY.invoke(info, target.frequency());
        if (has(SET_NETWORK_ID)) SET_NETWORK_ID.invoke(info, NETWORK_ID);
        if (has(SET_STATE)) SET_STATE.invoke(info, SupplicantState.COMPLETED);
        try {
            if (SCORE_FIELD != null) SCORE_FIELD.setInt(info, SCORE);
        } catch (Throwable ignored) { }
        return info;
    }

    /** 附近网络列表：整份目标列表，逐条构造系统对象。 */
    List<ScanResult> scanResults(long elapsedMs) throws Exception {
        List<ScanResult> results = new ArrayList<>(targets.size());
        for (WifiSettings.Target target : targets) {
            ScanResult result = NEW_SCAN.newInstance(ssid(target.ssid()), target.bssid(), CAPABILITIES,
                    target.rssi(), target.frequency(), elapsedMs, 0, 0);
            // 这两个 setter 在部分 ROM 上不存在，缺了就保持构造时的取值。
            if (has(SCAN_STANDARD)) SCAN_STANDARD.invoke(result, ScanResult.WIFI_STANDARD_11AX);
            if (has(SCAN_CHANNEL_WIDTH)) SCAN_CHANNEL_WIDTH.invoke(result, CHANNEL_WIDTH_MHZ);
            results.add(result);
        }
        return results;
    }

    /**
     * 用 UTF-8 文本构造 WifiSsid。
     *
     * <p>用 {@code fromUtf8Text} 而不是 {@code fromString}：后者在非引号包裹时按**十六进制**
     * 解码，名称里出现 `a`–`f` 与数字混排时会被解读成别的内容。中文名称必须走 UTF-8 这条路。
     */
    private static WifiSsid ssid(String text) throws Exception {
        return (WifiSsid) FROM_UTF8.invoke(null, text);
    }
}
