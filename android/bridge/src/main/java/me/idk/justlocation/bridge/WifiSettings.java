package me.idk.justlocation.bridge;

import org.json.JSONArray;
import org.json.JSONObject;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;

/**
 * Wi-Fi 通道的配置解析结果：开关打开时要输出的目标列表与作用范围。
 *
 * <p>这个类**刻意不引用任何 Android 类**，也不直接依赖 `org.json`——它接收的是普通的
 * `Map` / `List`（由 {@link #fromJson} 从后台心跳转换而来）。原因是 Android 单元测试里的
 * `org.json` 只是一个会抛异常的空壳桩，直接用它会让解析逻辑完全无法测试；把解析写成
 * 纯数据操作之后，最容易出错的分支（该不该接管、作用范围、缺省值）都有测试钉住，
 * 只剩"构造系统对象"那一小段只能在真机上验证。
 *
 * <p>列表语义与原版一致：**第一条是当前连接的网络**，其余作为附近网络。
 * 缺 `wifi` 段、开关关闭、模式非法、列表为空或所有条目都没有名称时返回 {@code null}，
 * 表示不接管。
 */
final class WifiSettings {
    record Target(String ssid, String bssid, int rssi, int linkSpeed, int frequency) {}

    private final SessionSnapshot scope;
    private final List<Target> targets;

    private WifiSettings(SessionSnapshot scope, List<Target> targets) {
        this.scope = scope;
        this.targets = List.copyOf(targets);
    }

    /** 把心跳文本转成普通数据结构，之后的解析完全不碰 JSON 库。 */
    static Map<String, Object> fromJson(String response) throws Exception {
        if (response == null) return null;
        return convert(new JSONObject(response));
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> convert(JSONObject object) {
        Map<String, Object> result = new java.util.LinkedHashMap<>();
        for (var keys = object.keys(); keys.hasNext(); ) {
            String key = keys.next();
            Object value = object.opt(key);
            result.put(key, value instanceof JSONObject nested ? convert(nested)
                    : value instanceof JSONArray array ? convert(array) : value);
        }
        return result;
    }

    private static List<Object> convert(JSONArray array) {
        List<Object> result = new ArrayList<>(array.length());
        for (int i = 0; i < array.length(); i++) {
            Object value = array.opt(i);
            result.add(value instanceof JSONObject nested ? convert(nested)
                    : value instanceof JSONArray items ? convert(items) : value);
        }
        return result;
    }

    static WifiSettings parse(Map<String, Object> response, long nowMs) {
        if (response == null) return null;
        if (number(response.get("version")) != 1 || !Boolean.TRUE.equals(response.get("ok"))) return null;
        Map<String, Object> state = map(response.get("state"));
        if (state == null || !Boolean.TRUE.equals(state.get("requested_active"))) return null;
        Map<String, Object> wifi = map(state.get("wifi"));
        if (wifi == null || !Boolean.TRUE.equals(wifi.get("enabled"))) return null;
        List<Object> list = list(wifi.get("targets"));
        if (list == null || list.isEmpty()) return null;
        Map<String, Object> config = map(state.get("config"));
        Map<String, Object> selection = config == null ? null : map(config.get("scope"));
        if (selection == null) return null;
        Object modeValue = selection.get("mode");
        String mode = modeValue instanceof String text ? text : null;
        if (mode == null) return null;
        HashSet<String> packages = new HashSet<>();
        if (mode.equals("apps")) {
            List<Object> names = list(selection.get("packages"));
            if (names == null) return null;
            for (Object name : names) if (name instanceof String text) packages.add(text);
        } else if (!mode.equals("all")) return null;
        List<Target> parsed = new ArrayList<>();
        for (Object entry : list) {
            Map<String, Object> item = map(entry);
            if (item == null) continue;
            // 没有名称的条目不能成为"已连接的"网络，直接跳过；其余字段缺省时沿用原版模型取值。
            String ssid = text(item.get("ssid"));
            if (ssid == null || ssid.trim().isEmpty()) continue;
            parsed.add(new Target(ssid.trim(), text(item.get("bssid")) == null ? "" : text(item.get("bssid")).trim(),
                    number(item.get("rssi"), 200), number(item.get("link_speed"), 866), number(item.get("frequency"), 5745)));
        }
        if (parsed.isEmpty()) return null;
        return new WifiSettings(new SessionSnapshot(true, mode.equals("all"), packages, nowMs), parsed);
    }

    SessionSnapshot scope() { return scope; }
    List<Target> targets() { return targets; }

    /**
     * 系统在 Wi-Fi 关闭时返回的占位名称（`WifiSsid.NONE`，编译期不可见的常量，这里用字面量）。
     * 读到它说明并没有真实连接，这时不该接管。
     */
    static boolean isPlaceholder(String ssid) {
        return ssid == null || ssid.isEmpty() || ssid.equals("<unknown ssid>");
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> map(Object value) {
        return value instanceof Map<?, ?> ? (Map<String, Object>) value : null;
    }

    @SuppressWarnings("unchecked")
    private static List<Object> list(Object value) {
        return value instanceof List<?> ? (List<Object>) value : null;
    }

    private static String text(Object value) { return value instanceof String string ? string : null; }

    private static int number(Object value, int fallback) {
        return value instanceof Number number ? number.intValue() : fallback;
    }

    /** 版本号等必需字段的取值：缺失时返回哨兵值，让上面的版本校验直接失败。 */
    private static int number(Object value) {
        return value instanceof Number number ? number.intValue() : Integer.MIN_VALUE;
    }
}
