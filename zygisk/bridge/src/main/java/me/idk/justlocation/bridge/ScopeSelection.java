package me.idk.justlocation.bridge;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import org.json.JSONObject;

/** One scope parser for location, satellite, cell, SIM, Wi-Fi and step channels. */
record ScopeSelection(boolean all, Set<String> packages) {
    ScopeSelection {
        packages = Set.copyOf(packages);
    }

    // A null feature selects the effective position/route scope published by the daemon.
    static ScopeSelection read(Map<?, ?> state, String feature) {
        Object container = feature != null && state.containsKey("scopes") ? state.get("scopes")
                                                                          : state.get("config");
        String key = feature != null && state.containsKey("scopes") ? feature : "scope";
        return container instanceof Map < ?, ? > values ? parse(values.get(key)) : null;
    }

    static ScopeSelection read(JSONObject state, String feature) throws Exception {
        boolean separate = feature != null && state.has("scopes");
        JSONObject container = state.optJSONObject(separate ? "scopes" : "config");
        JSONObject selection =
                container == null ? null : container.optJSONObject(separate ? feature : "scope");
        if (selection == null)
            return null;
        Object mode = selection.opt("mode");
        if (!(mode instanceof String))
            return null;
        List<Object> names = new ArrayList<>();
        var list = selection.optJSONArray("packages");
        if (list != null)
            for (int i = 0; i < list.length(); i++)
                names.add(list.opt(i));
        return parse(Map.of("mode", mode, "packages", names));
    }

    private static ScopeSelection parse(Object value) {
        if (!(value instanceof Map<?, ?> selection))
            return null;
        if ("all".equals(selection.get("mode")))
            return new ScopeSelection(true, Set.of());
        if (!"apps".equals(selection.get("mode"))
                || !(selection.get("packages") instanceof List<?> names) || names.isEmpty())
            return null;
        Set<String> packages = new HashSet<>();
        for (Object name : names) {
            if (!(name instanceof String text) || text.isBlank())
                return null;
            packages.add(text);
        }
        return new ScopeSelection(false, packages);
    }

    SessionSnapshot snapshot(long nowMs) {
        return new SessionSnapshot(true, all, packages, nowMs);
    }
}
