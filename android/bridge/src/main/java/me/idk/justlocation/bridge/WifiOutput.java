package me.idk.justlocation.bridge;

import android.net.wifi.ScanResult;
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

    private static final java.lang.reflect.Constructor<WifiInfo> NEW_INFO;      // WifiInfo()
    private static final java.lang.reflect.Constructor<ScanResult> NEW_SCAN;    // ScanResult(WifiSsid,String,String,int,int,long,int,int)
    private static final Method SET_SSID;         // WifiInfo.setSSID(WifiSsid)
    private static final Method SET_BSSID;        // WifiInfo.setBSSID(String)
    private static final Method SET_RSSI;         // WifiInfo.setRssi(int)
    private static final Method SET_LINK_SPEED;   // WifiInfo.setLinkSpeed(int)
    private static final Method SET_FREQUENCY;    // WifiInfo.setFrequency(int)
    private static final Method FROM_UTF8;        // WifiSsid.fromUtf8Text(CharSequence)
    private static final Method SCAN_SSID;        // ScanResult.setWifiSsid(WifiSsid)
    private static final Method SCAN_STANDARD;    // ScanResult.setWifiStandard(int)
    private static final Method SCAN_CHANNEL_WIDTH; // ScanResult.setChannelWidth(int)

    static {
        java.lang.reflect.Constructor<WifiInfo> newInfo = null;
        java.lang.reflect.Constructor<ScanResult> newScan = null;
        Method setSsid = null, setBssid = null, setRssi = null, setLinkSpeed = null, setFrequency = null;
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
        try { setRssi = WifiInfo.class.getMethod("setRssi", int.class); } catch (Throwable ignored) { }
        try { setLinkSpeed = WifiInfo.class.getMethod("setLinkSpeed", int.class); } catch (Throwable ignored) { }
        try { setFrequency = WifiInfo.class.getMethod("setFrequency", int.class); } catch (Throwable ignored) { }
        try { scanStandard = ScanResult.class.getMethod("setWifiStandard", int.class); } catch (Throwable ignored) { }
        try { scanWidth = ScanResult.class.getMethod("setChannelWidth", int.class); } catch (Throwable ignored) { }
        NEW_INFO = newInfo; NEW_SCAN = newScan;
        SET_SSID = setSsid; SET_BSSID = setBssid; SET_RSSI = setRssi;
        SET_LINK_SPEED = setLinkSpeed; SET_FREQUENCY = setFrequency;
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
     * <p>脱敏沿用平台策略：调用方自己的 {@link WifiInfo#getApplicableRedactions()} 是
     * `REDACT_FOR_ACCESS_FINE_LOCATION | REDACT_FOR_LOCAL_MAC_ADDRESS |
     * REDACT_FOR_NETWORK_SETTINGS`，把同样的位应用到合成对象上，编码方式与真实对象一致。
     * 这里**不能**用系统返回对象的脱敏位——那是服务端按调用方权限算过的，
     * 套到合成对象上会把合成 SSID 一起抹掉。
     */
    WifiInfo connectionInfo() throws Exception {
        WifiSettings.Target target = targets.get(0);
        WifiInfo info = NEW_INFO.newInstance();
        SET_SSID.invoke(info, ssid(target.ssid()));
        if (!target.bssid().isEmpty() && has(SET_BSSID)) SET_BSSID.invoke(info, target.bssid());
        if (has(SET_RSSI)) SET_RSSI.invoke(info, target.rssi());
        if (has(SET_LINK_SPEED)) SET_LINK_SPEED.invoke(info, target.linkSpeed());
        if (has(SET_FREQUENCY)) SET_FREQUENCY.invoke(info, target.frequency());
        return info.makeCopy(info.getApplicableRedactions());
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
