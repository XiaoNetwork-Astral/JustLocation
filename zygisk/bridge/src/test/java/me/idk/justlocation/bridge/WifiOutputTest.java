package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import org.junit.Test;

/**
 * Validate Wi-Fi settings and scope with plain Java collections, independently of Android object
 * construction.
 */
public class WifiOutputTest {
    private static final long NOW = 10_000;

    private static Map<String, Object> map(Object... pairs) {
        Map<String, Object> result = new LinkedHashMap<>();
        for (int i = 0; i < pairs.length; i += 2)
            result.put((String) pairs[i], pairs[i + 1]);
        return result;
    }

    private static Map<String, Object> target(String ssid, String bssid) {
        return map("id", "w-" + ssid, "ssid", ssid, "bssid", bssid);
    }

    /** Construct a minimal heartbeat. */
    private static Map<String, Object> state(
            Map<String, Object> wifi, Map<String, Object> scope, boolean active) {
        return map("version", 1, "ok", true, "state",
                map("requested_active", active, "config",
                        map("position", map("latitude", 1.0, "longitude", 2.0), "scope", scope),
                        "wifi", wifi));
    }

    private static Map<String, Object> apps(String... packages) {
        return map("mode", "apps", "packages", List.of(packages));
    }

    private static Map<String, Object> all() {
        return map("mode", "all");
    }

    private static Map<String, Object> twoTargets() {
        List<Object> targets = new ArrayList<>();
        targets.add(target("家里", "aa:bb:cc:dd:ee:ff"));
        targets.add(target("Coffee", "11:22:33:44:55:66"));
        return map("enabled", true, "targets", targets);
    }

    @Test
    public void missingRuntimeApisKeepTheWholeChannelOff() throws Exception {
        // Without framework APIs, the channel must be unavailable. Inspect reflectively without
        // requiring successful WifiOutput initialization.
        boolean apisMissing = true;
        for (String name :
                new String[] {"NEW_INFO", "NEW_SCAN", "SET_SSID", "FROM_UTF8", "SCAN_SSID"}) {
            try {
                var field = WifiOutput.class.getDeclaredField(name);
                field.setAccessible(true);
                if (field.get(null) != null)
                    apisMissing = false;
            } catch (Throwable error) {
                // An unavailable field is treated as a missing API.
            }
        }
        assertTrue(
                "Hidden APIs must be unavailable without Android framework classes", apisMissing);
        // Settings parsing remains independent of runtime object APIs.
        assertNotNull("Settings parsing must not depend on runtime APIs",
                WifiSettings.parse(state(twoTargets(), all(), true), NOW));
    }

    @Test
    public void disabledOrEmptyConfigurationDoesNotTakeOver() {
        assertNull(WifiSettings.parse(
                state(map("enabled", false, "targets", List.of(target("家", "aa:bb:cc:dd:ee:ff"))),
                        all(), true),
                NOW));
        assertNull(WifiSettings.parse(
                state(map("enabled", true, "targets", List.of()), all(), true), NOW));
        // A stopped session must pass through system output.
        assertNull(WifiSettings.parse(state(twoTargets(), all(), false), NOW));
        // An absent Wi-Fi section must leave the channel disabled.
        assertNull(WifiSettings.parse(
                map("version", 1, "ok", true, "state",
                        map("requested_active", true, "config", map("scope", all()))),
                NOW));
    }

    @Test
    public void theFirstTargetIsTheConnectedOneAndTheRestAreNearby() {
        var settings = WifiSettings.parse(state(twoTargets(), all(), true), NOW);
        assertNotNull(settings);
        assertEquals(2, settings.targets().size());
        assertEquals("家里", settings.targets().get(0).ssid());
        assertEquals("aa:bb:cc:dd:ee:ff", settings.targets().get(0).bssid());
        // Retain default field values in the parsed network.
        assertEquals(200, settings.targets().get(0).rssi());
        assertEquals(866, settings.targets().get(0).linkSpeed());
        assertEquals(5745, settings.targets().get(0).frequency());
        assertEquals("Coffee", settings.targets().get(1).ssid());
    }

    @Test
    public void explicitSignalFieldsAreKept() {
        List<Object> targets = new ArrayList<>();
        targets.add(map("id", "w1", "ssid", "公司", "bssid", "aa:bb:cc:dd:ee:ff", "rssi", -55,
                "link_speed", 433, "frequency", 2412));
        var settings = WifiSettings.parse(
                state(map("enabled", true, "targets", targets), all(), true), NOW);
        assertNotNull(settings);
        assertEquals(-55, settings.targets().get(0).rssi());
        assertEquals(433, settings.targets().get(0).linkSpeed());
        assertEquals(2412, settings.targets().get(0).frequency());
    }

    @Test
    public void entriesWithoutANameAreSkippedInsteadOfBecomingConnectedNetworks() {
        List<Object> targets = new ArrayList<>();
        targets.add(target("   ", ""));
        targets.add(target("公司", "aa:bb:cc:dd:ee:ff"));
        var settings = WifiSettings.parse(
                state(map("enabled", true, "targets", targets), all(), true), NOW);
        assertNotNull(settings);
        assertEquals(1, settings.targets().size());
        assertEquals("公司", settings.targets().get(0).ssid());
        // An all-empty network list is unusable.
        assertNull(WifiSettings.parse(
                state(map("enabled", true, "targets", List.of(target("", ""))), all(), true), NOW));
    }

    @Test
    public void scopeAndHeartbeatAgeDecideWhetherTheApplicationIsCovered() {
        var scoped = WifiSettings.parse(state(twoTargets(), apps("example.target"), true), NOW);
        assertNotNull(scoped);
        assertTrue(scoped.scope().appliesTo("example.target", NOW));
        assertFalse("Unselected apps must retain system output",
                scoped.scope().appliesTo("example.other", NOW));
        // Restore system output after the 20-second snapshot expiry.
        assertTrue("Snapshots remain active before expiry",
                scoped.scope().appliesTo("example.target", NOW + 19_000));
        assertFalse(scoped.scope().appliesTo("example.target", NOW + 21_000));

        var every = WifiSettings.parse(state(twoTargets(), all(), true), NOW);
        assertNotNull(every);
        assertTrue(every.scope().appliesTo("any.package", NOW));
    }

    @Test
    public void aBrokenScopeOrVersionIsRejected() {
        assertNull(WifiSettings.parse(state(twoTargets(), map("mode", "some"), true), NOW));
        assertNull(
                WifiSettings.parse(state(twoTargets(), map("packages", List.of("a")), true), NOW));
        assertNull(WifiSettings.parse(
                map("version", 2, "ok", true, "state", map("requested_active", true)), NOW));
        assertNull(WifiSettings.parse(
                map("version", 1, "ok", false, "state", map("requested_active", true)), NOW));
        assertNull(WifiSettings.parse(null, NOW));
    }

    @Test
    public void placeholderNamesMeanThereIsNoRealConnection() {
        // The disabled-Wi-Fi placeholder must not appear as a connection.
        assertTrue(WifiSettings.isPlaceholder(null));
        assertTrue(WifiSettings.isPlaceholder(""));
        assertTrue(WifiSettings.isPlaceholder("<unknown ssid>"));
        assertFalse(WifiSettings.isPlaceholder("家里"));
    }
}
