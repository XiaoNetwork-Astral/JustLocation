package me.idk.justlocation.bridge;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;

/** Read-only subscription mappings and card state; original permission checks run first. */
final class VirtualSubscriptionQueries {
    interface Source {
        VirtualSubscriptions get(String packageName, String attribution) throws Exception;
    }
    private final List<Method> methods = new ArrayList<>();
    private final Source source;
    private volatile boolean ready;

    VirtualSubscriptionQueries(Class<?> subscriptions, Class<?> phone, Source source)
            throws Exception {
        this.source = source;
        for (String name : List.of("getSlotIndex", "getSubId", "getPhoneId"))
            methods.add(subscriptions.getDeclaredMethod(name, int.class));
        for (String name : List.of("getDefaultSubId", "getDefaultDataSubId", "getDefaultVoiceSubId",
                     "getDefaultSmsSubId"))
            methods.add(subscriptions.getDeclaredMethod(name));
        methods.add(subscriptions.getDeclaredMethod("getActiveSubIdList", boolean.class));
        methods.add(subscriptions.getDeclaredMethod(
                "isActiveSubId", int.class, String.class, String.class));
        methods.add(phone.getDeclaredMethod("getSimStateForSlotIndex", int.class));
        methods.add(phone.getDeclaredMethod("hasIccCardUsingSlotIndex", int.class));
    }
    void install(MethodHook.Installer installer) throws Exception {
        for (Method method : methods)
            installer.install(method, call -> {
                boolean nested = SubscriptionQueries.insideOriginal();
                Object original = SubscriptionQueries.original(call);
                if (nested || !ready)
                    return original;
                String name = method.getName();
                boolean named = name.equals("isActiveSubId");
                VirtualSubscriptions virtuals =
                        source.get(named ? (String) call.arguments[2] : null,
                                named ? (String) call.arguments[3] : null);
                if (virtuals == null)
                    return original;
                if (name.startsWith("getDefault"))
                    return virtuals.defaultId((int) original);
                if (name.equals("getActiveSubIdList"))
                    return original == null ? null : virtuals.appendIds((int[]) original);
                int value = (int) call.arguments[1];
                return switch (name) {
                    case "getSlotIndex" -> virtuals.slot(value, (int) original);
                    case "getSubId" -> virtuals.id(value, (int) original);
                    // Android falls back to phone 0 for unknown IDs; match virtual IDs explicitly.
                    case "getPhoneId" -> {
                        if (value == Integer.MAX_VALUE)
                            value = virtuals.defaultId(-1);
                        var entry = virtuals.byId(value);
                        yield entry == null ? original : entry.slot();
                    }
                    case "isActiveSubId" -> (boolean) original || virtuals.byId(value) != null;
                    // LOADED is mapped to READY/PRESENT by the corresponding TelephonyManager APIs.
                    case "getSimStateForSlotIndex" ->
                        virtuals.bySlot(value) == null
                                        || ((int) original != 0 && (int) original != 1)
                                ? original
                                : 10;
                    case "hasIccCardUsingSlotIndex" ->
                        (boolean) original || virtuals.bySlot(value) != null;
                    default -> original;
                };
            });
        ready = true;
    }
}
