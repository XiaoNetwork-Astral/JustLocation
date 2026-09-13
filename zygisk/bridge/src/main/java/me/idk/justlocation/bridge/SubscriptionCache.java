package me.idk.justlocation.bridge;

import java.lang.reflect.Method;
import java.util.Objects;

/** Invalidate application-side SubscriptionManager caches on change and process recovery. */
final class SubscriptionCache {
    private final Method invalidate;
    private boolean initialized;
    private Object previous;
    SubscriptionCache() throws Exception {
        invalidate = android.telephony.SubscriptionManager.class.getDeclaredMethod(
                "invalidateSubscriptionManagerServiceCaches");
        invalidate.setAccessible(true);
    }
    void update(Object version) throws Exception {
        if (!initialized || !Objects.equals(previous, version)) {
            invalidate.invoke(null);
            previous = version;
            initialized = true;
        }
    }
}
