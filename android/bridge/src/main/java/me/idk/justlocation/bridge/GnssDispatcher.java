package me.idk.justlocation.bridge;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Set;
import java.util.WeakHashMap;
import java.util.function.Function;
import java.util.function.LongSupplier;
import java.util.function.Supplier;

/** Per-provider adapter. The platform retains active, permission, executor and binder-death rules. */
final class GnssDispatcher {
    private final Set<Object> providers = Collections.newSetFromMap(new WeakHashMap<>());
    private final Class<?> listenerType;
    private final Method deliver, getIdentity, packageName, noteOp;
    private final Field appOps;
    private final Supplier<GnssListener.Output> output;
    private final LongSupplier clock;
    private final Function<GnssFrame, Object> status;
    private final Class<?> operationType;

    GnssDispatcher(Class<?> provider, Class<?> listener, Class<?> registration, Class<?> identity, Class<?> operationType,
                   Supplier<GnssListener.Output> output, LongSupplier clock, Function<GnssFrame, Object> status) throws Exception {
        this.listenerType = listener; this.output = output; this.clock = clock; this.status = status;
        deliver = method(provider, "deliverToListeners", Function.class);
        getIdentity = method(registration, "getIdentity"); packageName = identity.getMethod("getPackageName");
        appOps = provider.getDeclaredField("mAppOpsHelper"); appOps.setAccessible(true);
        noteOp = appOps.getType().getMethod("noteOpNoThrow", int.class, identity);
        this.operationType = operationType;
    }

    private Object operation(Object helper, Object identity) {
        return Proxy.newProxyInstance(operationType.getClassLoader(), new Class<?>[]{operationType}, (proxy, method, args) -> {
            if (method.getName().equals("operate")) {
                Object target = args[0];
                if (Proxy.isProxyClass(target.getClass()) && Proxy.getInvocationHandler(target) instanceof GnssListener handler
                        && handler.needsTick()) {
                    boolean permitted;
                    try { permitted = (boolean) noteOp.invoke(helper, 1, identity); }
                    catch (ReflectiveOperationException error) { return null; }
                    if (permitted) handler.tick();
                }
            } else if (method.getDeclaringClass() == Object.class) return switch (method.getName()) {
                case "equals" -> proxy == args[0]; case "hashCode" -> System.identityHashCode(proxy);
                default -> "JustLocation GNSS delivery";
            };
            // Android 15 ListenerOperation lifecycle defaults are all empty, void methods.
            return null;
        });
    }

    Object wrap(Object identity, Object listener) throws Exception {
        return new GnssListener(listenerType, listener, (String) packageName.invoke(identity), output, clock, status).proxy();
    }
    void track(Object provider) { synchronized (providers) { providers.add(provider); } }
    void dispatch() throws Exception {
        ArrayList<Object> snapshot;
        synchronized (providers) { snapshot = new ArrayList<>(providers); }
        for (Object provider : snapshot) {
            Object helper = appOps.get(provider);
            deliver.invoke(provider, (Function<Object, Object>) registration -> {
                try {
                    Object identity = getIdentity.invoke(registration);
                    // AppOps is checked at execution, after scope, so unrelated apps do not
                    // acquire a synthetic location-access indicator every second.
                    return operation(helper, identity);
                } catch (ReflectiveOperationException error) {
                    // A failed optional channel must not crash system_server's delivery thread.
                    return null;
                }
            });
        }
    }
    private static Method method(Class<?> type, String name, Class<?>... args) throws NoSuchMethodException {
        for (Class<?> current = type; current != null; current = current.getSuperclass()) {
            try { Method method = current.getDeclaredMethod(name, args); method.setAccessible(true); return method; }
            catch (NoSuchMethodException ignored) { }
        }
        throw new NoSuchMethodException(type.getName() + "." + name);
    }
}
