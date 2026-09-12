package me.idk.justlocation.bridge;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Function;

/** Replace operator fields only on subscription records authorized for the caller. */
final class SubscriptionQueries {
    interface Output {
        Object replace(Object authorizedRecord) throws Exception;
    }
    private record Query(Method method, int packageIndex) {}
    private final List<Query> queries;
    private final Function<String, Output> source;
    private volatile boolean ready;

    SubscriptionQueries(Class<?> service, Function<String, Output> source) throws Exception {
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
                new Query(service.getDeclaredMethod("getActiveSubscriptionInfoForIccId",
                                  String.class, String.class, String.class),
                        2));
    }

    void install(MethodHook.Installer installer) throws Exception {
        for (Query query : queries)
            installer.install(query.method, call -> {
                // Includes permission, AppOps, user/profile filtering and identifier redaction.
                Object original = call.original();
                if (!ready || original == null)
                    return original;
                Output output = source.apply((String) call.arguments[query.packageIndex]);
                if (output == null)
                    return original;
                if (original instanceof List<?> records) {
                    if (records.isEmpty())
                        return original;
                    List<Object> result = new ArrayList<>(records.size());
                    for (Object record : records)
                        result.add(output.replace(record));
                    return result;
                }
                return output.replace(original);
            });
        ready = true;
    }
}
