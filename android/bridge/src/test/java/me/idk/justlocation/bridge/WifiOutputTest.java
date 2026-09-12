package me.idk.justlocation.bridge;

import org.junit.Test;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import static org.junit.Assert.*;

/**
 * Wi-Fi 通道里不依赖 Android 对象的那部分：配置解析与作用范围门控。
 *
 * <p>解析器接收的是普通的 `Map` / `List`（见 {@link WifiSettings}），因此这里可以直接喂数据；
 * 合成对象的构造要 `WifiInfo` / `ScanResult` / `WifiSsid` 的隐藏入口，只有真机才有，
 * 不在这里覆盖。"什么情况下该接管、什么情况下必须放行系统数据"是这条通道最容易出错的地方，
 * 全部在下面钉住。
 */
public class WifiOutputTest {
    private static final long NOW = 10_000;

    private static Map<String, Object> map(Object... pairs) {
        Map<String, Object> result = new LinkedHashMap<>();
        for (int i = 0; i < pairs.length; i += 2) result.put((String) pairs[i], pairs[i + 1]);
        return result;
    }

    private static Map<String, Object> target(String ssid, String bssid) {
        return map("id", "w-" + ssid, "ssid", ssid, "bssid", bssid);
    }

    /** 组装一份最小的心跳。 */
    private static Map<String, Object> state(Map<String, Object> wifi, Map<String, Object> scope, boolean active) {
        return map("version", 1, "ok", true,
                "state", map("requested_active", active,
                        "config", map("position", map("latitude", 1.0, "longitude", 2.0), "scope", scope),
                        "wifi", wifi));
    }

    private static Map<String, Object> apps(String... packages) {
        return map("mode", "apps", "packages", List.of(packages));
    }

    private static Map<String, Object> all() { return map("mode", "all"); }

    private static Map<String, Object> twoTargets() {
        List<Object> targets = new ArrayList<>();
        targets.add(target("家里", "aa:bb:cc:dd:ee:ff"));
        targets.add(target("Coffee", "11:22:33:44:55:66"));
        return map("enabled", true, "targets", targets);
    }

    @Test public void missingRuntimeApisKeepTheWholeChannelOff() throws Exception {
        // 没有 framework 时整条通道必须关闭，而且不能把异常抛给调用方。
        // 这里用反射读静态字段，避免触发 WifiOutput 的类初始化（那在 JVM 上本就会失败）。
        boolean apisMissing = true;
        for (String name : new String[]{"NEW_INFO", "NEW_SCAN", "SET_SSID", "FROM_UTF8", "SCAN_SSID"}) {
            try {
                var field = WifiOutput.class.getDeclaredField(name);
                field.setAccessible(true);
                if (field.get(null) != null) apisMissing = false;
            } catch (Throwable error) {
                // 读不到就当作"缺失"，这正是期望的行为。
            }
        }
        assertTrue("测试 JVM 里没有 framework，隐藏入口必须判定为缺失", apisMissing);
        // 纯解析不受运行时接口影响：配置本身照样能读出来。
        assertNotNull("纯解析不受运行时接口影响", WifiSettings.parse(state(twoTargets(), all(), true), NOW));
    }

    @Test public void disabledOrEmptyConfigurationDoesNotTakeOver() {
        assertNull(WifiSettings.parse(state(map("enabled", false, "targets", List.of(target("家", "aa:bb:cc:dd:ee:ff"))), all(), true), NOW));
        assertNull(WifiSettings.parse(state(map("enabled", true, "targets", List.of()), all(), true), NOW));
        // 心停止时（requested_active=false）不接管。
        assertNull(WifiSettings.parse(state(twoTargets(), all(), false), NOW));
        // 没有 wifi 段（旧后台）也不接管。
        assertNull(WifiSettings.parse(map("version", 1, "ok", true,
                "state", map("requested_active", true, "config", map("scope", all()))), NOW));
    }

    @Test public void theFirstTargetIsTheConnectedOneAndTheRestAreNearby() {
        var settings = WifiSettings.parse(state(twoTargets(), all(), true), NOW);
        assertNotNull(settings);
        assertEquals(2, settings.targets().size());
        assertEquals("家里", settings.targets().get(0).ssid());
        assertEquals("aa:bb:cc:dd:ee:ff", settings.targets().get(0).bssid());
        // 原版模型缺省值随条目一起带出，界面与日志据此显示。
        assertEquals(200, settings.targets().get(0).rssi());
        assertEquals(866, settings.targets().get(0).linkSpeed());
        assertEquals(5745, settings.targets().get(0).frequency());
        assertEquals("Coffee", settings.targets().get(1).ssid());
    }

    @Test public void explicitSignalFieldsAreKept() {
        List<Object> targets = new ArrayList<>();
        targets.add(map("id", "w1", "ssid", "公司", "bssid", "aa:bb:cc:dd:ee:ff",
                "rssi", -55, "link_speed", 433, "frequency", 2412));
        var settings = WifiSettings.parse(state(map("enabled", true, "targets", targets), all(), true), NOW);
        assertNotNull(settings);
        assertEquals(-55, settings.targets().get(0).rssi());
        assertEquals(433, settings.targets().get(0).linkSpeed());
        assertEquals(2412, settings.targets().get(0).frequency());
    }

    @Test public void entriesWithoutANameAreSkippedInsteadOfBecomingConnectedNetworks() {
        List<Object> targets = new ArrayList<>();
        targets.add(target("   ", ""));
        targets.add(target("公司", "aa:bb:cc:dd:ee:ff"));
        var settings = WifiSettings.parse(state(map("enabled", true, "targets", targets), all(), true), NOW);
        assertNotNull(settings);
        assertEquals(1, settings.targets().size());
        assertEquals("公司", settings.targets().get(0).ssid());
        // 全是空名称时整条配置不可用，而不是接管后返回一个空网络。
        assertNull(WifiSettings.parse(state(map("enabled", true, "targets", List.of(target("", ""))), all(), true), NOW));
    }

    @Test public void scopeAndHeartbeatAgeDecideWhetherTheApplicationIsCovered() {
        var scoped = WifiSettings.parse(state(twoTargets(), apps("example.target"), true), NOW);
        assertNotNull(scoped);
        assertTrue(scoped.scope().appliesTo("example.target", NOW));
        assertFalse("不在名单里的应用必须拿到系统数据", scoped.scope().appliesTo("example.other", NOW));
        // 心跳过期（超过 20 秒，2026-09-12 实测后从 3 秒放宽）后恢复系统输出。
        assertTrue("窗口内仍然接管", scoped.scope().appliesTo("example.target", NOW + 19_000));
        assertFalse(scoped.scope().appliesTo("example.target", NOW + 21_000));

        var every = WifiSettings.parse(state(twoTargets(), all(), true), NOW);
        assertNotNull(every);
        assertTrue(every.scope().appliesTo("any.package", NOW));
    }

    @Test public void aBrokenScopeOrVersionIsRejected() {
        assertNull(WifiSettings.parse(state(twoTargets(), map("mode", "some"), true), NOW));
        assertNull(WifiSettings.parse(state(twoTargets(), map("packages", List.of("a")), true), NOW));
        assertNull(WifiSettings.parse(map("version", 2, "ok", true, "state", map("requested_active", true)), NOW));
        assertNull(WifiSettings.parse(map("version", 1, "ok", false, "state", map("requested_active", true)), NOW));
        assertNull(WifiSettings.parse(null, NOW));
    }

    @Test public void placeholderNamesMeanThereIsNoRealConnection() {
        // Wi-Fi 关闭时系统返回的占位名不能被当成"已连接"。
        assertTrue(WifiSettings.isPlaceholder(null));
        assertTrue(WifiSettings.isPlaceholder(""));
        assertTrue(WifiSettings.isPlaceholder("<unknown ssid>"));
        assertFalse(WifiSettings.isPlaceholder("家里"));
    }
}
