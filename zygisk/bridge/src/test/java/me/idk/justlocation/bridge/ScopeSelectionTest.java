package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import java.util.List;
import java.util.Map;

import org.junit.Test;

public class ScopeSelectionTest {
    private Map<String, Object> apps(String name) {
        return Map.of("mode", "apps", "packages", List.of(name));
    }
    @Test
    public void effectiveMotionWifiAndSimSelectionsAreIndependentAndExpireTogether() {
        var state = Map.of("config", Map.of("scope", apps("example.route")), "scopes",
                Map.of("position", apps("example.position"), "route", apps("example.route"), "wifi",
                        apps("example.wifi"), "sim", apps("example.sim")));
        for (String feature : new String[] {null, "wifi", "sim"}) {
            String selected = "example." + (feature == null ? "route" : feature);
            SessionSnapshot scope = ScopeSelection.read(state, feature).snapshot(1000);
            assertTrue(scope.appliesTo(selected, 20_999));
            assertFalse(scope.appliesTo(selected, 21_000));
            assertFalse(scope.appliesTo("example.position", 1000));
            assertFalse(scope.appliesTo(null, 1000));
            assertFalse(scope.appliesTo("", 1000));
        }
    }
    @Test
    public void legacyFallbackNeverMasksMalformedNewSelections() {
        var config = Map.of("scope", apps("example.legacy"));
        assertTrue(ScopeSelection.read(Map.of("config", config), "wifi")
                        .snapshot(0)
                        .appliesTo("example.legacy", 1));
        for (Object invalid : List.of(Map.of(), "invalid",
                     Map.of("wifi", Map.of("mode", "unknown")), Map.of("wifi", apps(" ")),
                     Map.of("wifi", Map.of("mode", "apps", "packages", List.of(7))))) {
            assertNull(ScopeSelection.read(Map.of("config", config, "scopes", invalid), "wifi"));
        }
        assertFalse(
                ScopeSelection.read(Map.of("scopes", Map.of("sim", Map.of("mode", "all"))), "sim")
                        .snapshot(0)
                        .appliesTo(" ", 1));
    }
}
