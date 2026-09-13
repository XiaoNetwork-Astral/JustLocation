package me.idk.justlocation.bridge;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Function;

/** Replace operator fields only on subscription records authorized for the caller. */
final class SubscriptionQueries {
    interface Output {
        Object replace(Object authorizedRecord) throws Exception;
        default VirtualSubscriptions virtuals() {
            return null;
        }
        default List<?> additions(List<?> records) {
            return List.of();
        }
    }
    interface Source {
        Output get(String packageName, String attribution) throws Exception;
    }
    private record Query(Method method, int packageIndex) {}
    private final List<Query> queries;
    private final Source source;
    private volatile boolean ready;
    private static final ThreadLocal<Boolean> ORIGINAL = ThreadLocal.withInitial(() -> false);

    static boolean insideOriginal() {
        return ORIGINAL.get();
    }
    static Object original(MethodHook.Call call) throws Throwable {
        boolean before = ORIGINAL.get();
        ORIGINAL.set(true);
        try {
            return call.original();
        } finally {
            ORIGINAL.set(before);
        }
    }

    SubscriptionQueries(Class<?> service, Function<String, Output> source) throws Exception {
        this(service, (pkg, attribution) -> source.apply(pkg));
    }
    SubscriptionQueries(Class<?> service, Source source) throws Exception {
        this.source = source;
        queries = List.of(new Query(service.getDeclaredMethod(
                                            "getAllSubInfoList", String.class, String.class),
                                  1),
                new Query(service.getDeclaredMethod("getActiveSubscriptionInfoList", String.class,
                                  String.class, boolean.class),
                        1),
                new Query(service.getDeclaredMethod(
                                  "getAvailableSubscriptionInfoList", String.class, String.class),
                        1),
                new Query(service.getDeclaredMethod(
                                  "getAccessibleSubscriptionInfoList", String.class),
                        1),
                new Query(service.getDeclaredMethod(
                                  "getOpportunisticSubscriptions", String.class, String.class),
                        1),
                new Query(service.getDeclaredMethod("getActiveSubscriptionInfo", int.class,
                                  String.class, String.class),
                        2),
                new Query(service.getDeclaredMethod("getActiveSubscriptionInfoForSimSlotIndex",
                                  int.class, String.class, String.class),
                        2),
                new Query(service.getDeclaredMethod("getActiveSubInfoCount", String.class,
                                  String.class, boolean.class),
                        1),
                new Query(service.getDeclaredMethod("getActiveSubscriptionInfoForIccId",
                                  String.class, String.class, String.class),
                        2));
    }

    void install(MethodHook.Installer installer) throws Exception {
        for (Query query : queries)
            installer.install(query.method, call -> {
                // Includes permission, AppOps, user/profile filtering and identifier redaction.
                boolean nested = insideOriginal();
                Object original = original(call);
                if (!ready || nested)
                    return original;
                int index = query.packageIndex;
                Output output = source.get((String) call.arguments[index],
                        index + 1 < call.arguments.length ? (String) call.arguments[index + 1]
                                                          : null);
                if (output == null)
                    return original;
                String name = query.method.getName();
                VirtualSubscriptions virtuals = output.virtuals();
                if (name.equals("getActiveSubInfoCount"))
                    return (int) original + (virtuals == null ? 0 : virtuals.size());
                if (original instanceof List<?> records) {
                    List<Object> result = new ArrayList<>(records.size());
                    for (Object record : records)
                        result.add(output.replace(record));
                    if (virtuals != null
                            && (name.equals("getAllSubInfoList")
                                    || name.equals("getActiveSubscriptionInfoList")
                                    || name.equals("getAvailableSubscriptionInfoList")))
                        result.addAll(output.additions(records));
                    return result;
                }
                if (original != null)
                    return output.replace(original);
                if (virtuals == null)
                    return null;
                VirtualSubscriptions.Entry entry = switch (name) {
                    case "getActiveSubscriptionInfo" -> virtuals.byId((int) call.arguments[1]);
                    case "getActiveSubscriptionInfoForSimSlotIndex" ->
                        virtuals.bySlot((int) call.arguments[1]);
                    default -> null;
                };
                return entry == null ? null : entry.info();
            });
        ready = true;
    }
}
