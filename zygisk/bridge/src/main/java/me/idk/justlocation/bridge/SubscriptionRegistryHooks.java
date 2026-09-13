package me.idk.justlocation.bridge;

import android.os.SystemClock;
import android.util.Log;

import java.lang.reflect.Method;
import java.util.function.Supplier;

/** Access the local registry, including listeners registered before the bridge became ready. */
final class SubscriptionRegistryHooks {
    private final Class<?> registryType;
    private final Method getService;
    private final SubscriptionRegistry registry;
    private final SubscriptionCache cache;
    private final Supplier<TelephonySnapshot> snapshot;
    private Object service;
    private final SubscriptionPublication publication = new SubscriptionPublication();

    private SubscriptionRegistryHooks(ClassLoader loader, Supplier<TelephonySnapshot> snapshot)
            throws Exception {
        this.snapshot = snapshot;
        cache = new SubscriptionCache();
        cache.update(null);
        registryType = Class.forName("com.android.server.TelephonyRegistry", false, loader);
        getService =
                Class.forName("android.os.ServiceManager").getMethod("getService", String.class);
        registry = new SubscriptionRegistry(registryType,
                Class.forName("com.android.server.TelephonyRegistry$Record", false, loader),
                Class.forName("com.android.internal.telephony.IOnSubscriptionsChangedListener",
                        false, loader),
                pkg -> {
                    TelephonySnapshot current = snapshot.get();
                    long now = SystemClock.elapsedRealtime();
                    return current != null && current.virtuals(pkg, now) != null
                            ? current.virtualVersion(now)
                            : null;
                });
    }
    static SubscriptionRegistryHooks install(
            ClassLoader loader, Supplier<TelephonySnapshot> snapshot) {
        try {
            return new SubscriptionRegistryHooks(loader, snapshot);
        } catch (Exception error) {
            Log.w("JustLocation", "Subscription change notifications unavailable", error);
            return null;
        }
    }
    boolean ready() {
        return service != null;
    }
    void observe(org.json.JSONObject state, long now) {
        publication.observe(state.isNull("virtual_sim_version")
                        ? null
                        : state.optString("virtual_sim_version", null),
                state.isNull("virtual_sim_applied") ? null
                                                    : state.optString("virtual_sim_applied", null),
                now);
    }
    void update() throws Exception {
        TelephonySnapshot current = snapshot.get();
        long now = SystemClock.elapsedRealtime();
        Object version = current == null ? null : current.virtualVersion(now);
        cache.update(version);
        if (service == null) {
            Object candidate = getService.invoke(null, "telephony.registry");
            if (registryType.isInstance(candidate))
                service = candidate;
        }
        if (service != null && publication.canNotify(version != null, now))
            registry.update(service);
    }
}
