package me.idk.justlocation.bridge;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;

import org.json.JSONArray;
import org.json.JSONObject;

/**
 * Parse Wi-Fi settings and scope using ordinary Java collections. The first network is the current
 * connection; SessionSnapshot provides the shared expiry rule.
 */
final class WifiSettings {
    record Target(String ssid, String bssid, int rssi, int linkSpeed, int frequency) {}

    private final SessionSnapshot scope;
    private final List<Target> targets;

    private WifiSettings(SessionSnapshot scope, List<Target> targets) {
        this.scope = scope;
        this.targets = List.copyOf(targets);
    }

    /** Convert heartbeat JSON to ordinary data before validating settings. */
    static Map<String, Object> fromJson(String response) throws Exception {
        if (response == null)
            return null;
        return convert(new JSONObject(response));
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> convert(JSONObject object) {
        Map<String, Object> result = new java.util.LinkedHashMap<>();
        for (var keys = object.keys(); keys.hasNext();) {
            String key = keys.next();
            Object value = object.opt(key);
            result.put(key,
                    value instanceof JSONObject nested         ? convert(nested)
                            : value instanceof JSONArray array ? convert(array)
                                                               : value);
        }
        return result;
    }

    private static List<Object> convert(JSONArray array) {
        List<Object> result = new ArrayList<>(array.length());
        for (int i = 0; i < array.length(); i++) {
            Object value = array.opt(i);
            result.add(value instanceof JSONObject nested      ? convert(nested)
                            : value instanceof JSONArray items ? convert(items)
                                                               : value);
        }
        return result;
    }

    static WifiSettings parse(Map<String, Object> response, long nowMs) {
        if (response == null)
            return null;
        if (number(response.get("version")) != 1 || !Boolean.TRUE.equals(response.get("ok")))
            return null;
        Map<String, Object> state = map(response.get("state"));
        if (state == null || !Boolean.TRUE.equals(state.get("requested_active")))
            return null;
        Map<String, Object> wifi = map(state.get("wifi"));
        if (wifi == null || !Boolean.TRUE.equals(wifi.get("enabled")))
            return null;
        List<Object> list = list(wifi.get("targets"));
        if (list == null || list.isEmpty())
            return null;
        Map<String, Object> config = map(state.get("config"));
        Map<String, Object> selection = config == null ? null : map(config.get("scope"));
        if (selection == null)
            return null;
        Object modeValue = selection.get("mode");
        String mode = modeValue instanceof String text ? text : null;
        if (mode == null)
            return null;
        HashSet<String> packages = new HashSet<>();
        if (mode.equals("apps")) {
            List<Object> names = list(selection.get("packages"));
            if (names == null)
                return null;
            for (Object name : names)
                if (name instanceof String text)
                    packages.add(text);
        } else if (!mode.equals("all"))
            return null;
        List<Target> parsed = new ArrayList<>();
        for (Object entry : list) {
            Map<String, Object> item = map(entry);
            if (item == null)
                continue;
            // Skip unnamed networks; absent optional fields use the configured defaults.
            String ssid = text(item.get("ssid"));
            if (ssid == null || ssid.trim().isEmpty())
                continue;
            parsed.add(new Target(ssid.trim(),
                    text(item.get("bssid")) == null ? "" : text(item.get("bssid")).trim(),
                    number(item.get("rssi"), 200), number(item.get("link_speed"), 866),
                    number(item.get("frequency"), 5745)));
        }
        if (parsed.isEmpty())
            return null;
        return new WifiSettings(
                new SessionSnapshot(true, mode.equals("all"), packages, nowMs), parsed);
    }

    SessionSnapshot scope() {
        return scope;
    }
    List<Target> targets() {
        return targets;
    }

    /**
     * WifiSsid.NONE is a hidden constant denoting no connection. Its placeholder must not trigger
     * simulated connection output.
     */
    static boolean isPlaceholder(String ssid) {
        return ssid == null || ssid.isEmpty() || ssid.equals("<unknown ssid>");
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> map(Object value) {
        return value instanceof Map < ?, ? > ? (Map<String, Object>) value : null;
    }

    @SuppressWarnings("unchecked")
    private static List<Object> list(Object value) {
        return value instanceof List<?> ? (List<Object>) value : null;
    }

    private static String text(Object value) {
        return value instanceof String string ? string : null;
    }

    private static int number(Object value, int fallback) {
        return value instanceof Number number ? number.intValue() : fallback;
    }

    /** Return a sentinel for missing required numeric fields so validation fails. */
    private static int number(Object value) {
        return value instanceof Number number ? number.intValue() : Integer.MIN_VALUE;
    }
}
