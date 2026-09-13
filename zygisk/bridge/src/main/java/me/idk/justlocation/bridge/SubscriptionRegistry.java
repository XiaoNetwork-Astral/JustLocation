package me.idk.justlocation.bridge;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.function.Function;

/** Notify the framework's registered listeners when their scoped virtual subscriptions change. */
final class SubscriptionRegistry {
    private final Field records, callback, packageName, uid;
    private final Method changed;
    private final Function<String, Object> version;
    private final Map<Object, Object> delivered = new IdentityHashMap<>();

    SubscriptionRegistry(Class<?> registry, Class<?> record, Class<?> listener,
            Function<String, Object> version) throws Exception {
        records = field(registry, "mRecords");
        callback = field(record, "onSubscriptionsChangedListenerCallback");
        packageName = field(record, "callingPackage");
        uid = field(record, "callerUid");
        changed = listener.getMethod("onSubscriptionsChanged");
        this.version = version;
    }
    private static Field field(Class<?> type, String name) throws Exception {
        Field field = type.getDeclaredField(name);
        field.setAccessible(true);
        return field;
    }
    void update(Object registry) throws Exception {
        List<?> listeners = (List<?>) records.get(registry);
        List<Object> notify = new ArrayList<>();
        synchronized (listeners) {
            delivered.keySet().retainAll(listeners);
            for (Object record : listeners) {
                Object target = callback.get(record);
                if (target == null || uid.getInt(record) % 100000 < 10000)
                    continue;
                Object next = version.apply((String) packageName.get(record));
                Object previous = delivered.get(record);
                if (!Objects.equals(previous, next)) {
                    notify.add(target);
                    if (next == null)
                        delivered.remove(record);
                    else
                        delivered.put(record, next);
                }
            }
        }
        // No registry lock is held across Binder callbacks. Real notifications remain untouched.
        for (Object target : notify) {
            try {
                changed.invoke(target);
            } catch (ReflectiveOperationException
                            deadListener) { /* Registry owns Binder death cleanup. */
            }
        }
    }
}
