package me.idk.justlocation.bridge;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Set;
import java.util.WeakHashMap;
import java.util.function.Function;
import java.util.function.LongSupplier;

/** Uses the platform's active registrations and delivery operations, never its global last fix. */
final class ProviderDispatcher {
    private final Set<Object> providers = Collections.newSetFromMap(new WeakHashMap<>());
    private final Method deliver, getName, getIdentity, getPackageName, accept;

    ProviderDispatcher(Class<?> provider, Class<?> registration, Class<?> result, Class<?> identity) throws Exception {
        deliver = method(provider, "deliverToListeners", Function.class);
        getName = provider.getMethod("getName");
        getIdentity = method(registration, "getIdentity");
        getPackageName = identity.getMethod("getPackageName");
        accept = method(registration, "acceptLocationChange", result);
    }

    void track(Object provider) {
        synchronized (providers) { providers.add(provider); }
    }

    void dispatch(SessionSnapshot scope, LongSupplier clock, Function<String, Object> result) throws Exception {
        ArrayList<Object> snapshot;
        synchronized (providers) { snapshot = new ArrayList<>(providers); }
        // Never hold the registry lock while taking a platform provider lock.
        for (Object provider : snapshot) {
            String name = (String) getName.invoke(provider);
            deliver.invoke(provider, (Function<Object, Object>) registration -> {
                try {
                    String caller = (String) getPackageName.invoke(getIdentity.invoke(registration));
                    if (!scope.appliesTo(caller, clock.getAsLong())) return null;
                    return accept.invoke(registration, result.apply(name));
                } catch (ReflectiveOperationException error) {
                    throw new IllegalStateException("Cannot prepare location delivery", error);
                }
            });
        }
    }

    private static Method method(Class<?> type, String name, Class<?>... parameters) throws NoSuchMethodException {
        for (Class<?> current = type; current != null; current = current.getSuperclass()) {
            try {
                Method method = current.getDeclaredMethod(name, parameters);
                method.setAccessible(true);
                return method;
            } catch (NoSuchMethodException ignored) { }
        }
        throw new NoSuchMethodException(type.getName() + "." + name);
    }
}
