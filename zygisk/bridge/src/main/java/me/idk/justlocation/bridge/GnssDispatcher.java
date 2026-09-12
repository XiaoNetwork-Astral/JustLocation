package me.idk.justlocation.bridge;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Set;
import java.util.WeakHashMap;
import java.util.concurrent.Executor;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.BiFunction;
import java.util.function.Function;

/**
 * Per-provider listener adapter. Platform-driven channels use deliverToListeners; raw channels use
 * heartbeat-driven delivery without waiting for HAL callbacks. Both retain AppOps checks and
 * registration executors. GnssTick.needsTick controls scope and snapshot validity.
 */
final class GnssDispatcher {
    private final Set<Object> providers = Collections.newSetFromMap(new WeakHashMap<>());
    /**
     * Track wrappers registered through addListener separately from the platform's merged,
     * active-only registration table.
     */
    private final List<Entry> registrations = new ArrayList<>();
    private final Class<?> listenerType;
    private final Method deliver, getIdentity, packageName, noteOp;
    private final Field appOps;
    private Field executor, listener;
    private Method operate;
    private boolean reflected;
    /** Accept either status/NMEA or raw listener wrappers through the common GnssTick contract. */
    private final BiFunction<Object, String, Object> wrapper;
    private final Class<?> operationType;
    private final boolean selfDriven;

    /** Keep weak references so unregistered or dead clients do not remain delivery targets. */
    private record Entry(java.lang.ref.WeakReference<Object> target, Object identity) {}

    private final AtomicInteger applied = new AtomicInteger();
    private final AtomicInteger needs = new AtomicInteger();
    private final AtomicInteger ticks = new AtomicInteger();
    private final AtomicInteger delivered = new AtomicInteger();
    private final AtomicInteger failures = new AtomicInteger();
    private final AtomicInteger dispatchCalls = new AtomicInteger();
    /** Include the last failure in heartbeat diagnostics, where it survives logcat rotation. */
    private volatile String failure;
    /** Wrapper-reported tick and listener call counts, updated after each operation. */
    private volatile long wrapperTicks = -1, wrapperDeliveries = -1;
    /** Receiver identity reported by GnssTick.description. */
    private volatile String wrapperIdentity;

    GnssDispatcher(Class<?> provider, Class<?> listener, Class<?> registration, Class<?> identity,
            Class<?> operationType, BiFunction<Object, String, Object> wrapper) throws Exception {
        this(provider, listener, registration, identity, operationType, wrapper, false);
    }

    GnssDispatcher(Class<?> provider, Class<?> listener, Class<?> registration, Class<?> identity,
            Class<?> operationType, BiFunction<Object, String, Object> wrapper, boolean selfDriven)
            throws Exception {
        this.listenerType = listener;
        this.wrapper = wrapper;
        this.selfDriven = selfDriven;
        // Platform delivery is optional for self-driven channels and required otherwise.
        deliver = selfDriven ? optional(provider, "deliverToListeners", Function.class)
                             : method(provider, "deliverToListeners", Function.class);
        getIdentity = method(registration, "getIdentity");
        packageName = identity.getMethod("getPackageName");
        appOps = provider.getDeclaredField("mAppOpsHelper");
        appOps.setAccessible(true);
        noteOp = appOps.getType().getMethod("noteOpNoThrow", int.class, identity);
        this.operationType = operationType;
    }

    private Object operation(Object helper, Object identity) {
        return Proxy.newProxyInstance(operationType.getClassLoader(),
                new Class<?>[] {operationType}, (proxy, method, args) -> {
                    if (method.getName().equals("operate")) {
                        Object target = args[0];
                        if (Proxy.isProxyClass(target.getClass())
                                && Proxy.getInvocationHandler(target) instanceof GnssTick handler) {
                            needs.incrementAndGet();
                            if (handler.needsTick()) {
                                ticks.incrementAndGet();
                                // Record the receiver before delivery so failures still identify
                                // their target.
                                if (handler.description() != null)
                                    wrapperIdentity = handler.description();
                                boolean permitted;
                                try {
                                    permitted = (boolean) noteOp.invoke(helper, 1, identity);
                                } catch (ReflectiveOperationException error) {
                                    failure = describe(error);
                                    failures.incrementAndGet();
                                    return null;
                                }
                                if (permitted) {
                                    handler.tick();
                                    wrapperTicks = handler.ticks();
                                    wrapperDeliveries = handler.deliveries();
                                }
                            }
                        }
                    } else if (method.getDeclaringClass() == Object.class)
                        return switch (method.getName()) {
                            case "equals" -> proxy == args[0];
                            case "hashCode" -> System.identityHashCode(proxy);
                            default -> "JustLocation GNSS delivery";
                        };
                    // Android 15 ListenerOperation lifecycle defaults are all empty, void methods.
                    return null;
                });
    }

    Object wrap(Object identity, Object listener) throws Exception {
        Object wrapped = wrapper.apply(listener, (String) packageName.invoke(identity));
        synchronized (registrations) {
            registrations.add(new Entry(new java.lang.ref.WeakReference<>(wrapped), identity));
        }
        return wrapped;
    }

    void track(Object provider) {
        synchronized (providers) {
            providers.add(provider);
        }
    }

    /**
     * Deliver through platform registrations or tracked wrappers, depending on the channel. AppOps
     * checks run in operate; scope and snapshot checks run in needsTick.
     */
    void dispatch() throws Exception {
        dispatchCalls.incrementAndGet();
        ArrayList<Object> snapshot;
        synchronized (providers) {
            snapshot = new ArrayList<>(providers);
        }
        if (selfDriven) {
            for (Object provider : snapshot)
                drive(appOps.get(provider));
            return;
        }
        if (deliver == null)
            return;
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
                    failures.incrementAndGet();
                    return null;
                }
            });
        }
    }

    /** Deliver to tracked wrappers independently of HAL callbacks and platform active flags. */
    private void drive(Object helper) throws Exception {
        ArrayList<Entry> entries;
        synchronized (registrations) {
            entries = new ArrayList<>(registrations);
        }
        int ready = 0;
        for (Entry entry : entries) {
            Object target = entry.target().get();
            // Discard wrappers whose clients have released them instead of repeatedly delivering
            // to dead listeners.
            if (target == null) {
                synchronized (registrations) {
                    registrations.remove(entry);
                }
                continue;
            }
            ready++;
            delivers(helper, target, entry.identity());
        }
        if (ready > 0)
            applied.incrementAndGet();
    }

    /** Prefer the registration's executor; use synchronous delivery when unavailable. */
    private void delivers(Object helper, Object target, Object identity) throws Exception {
        Object operation = operation(helper, identity);
        Executor executor;
        try {
            executor = executor(identity);
        } catch (RuntimeException error) {
            executor = null;
        }
        if (executor == null) {
            operate(helper, operation, target);
            return;
        }
        try {
            executor.execute(() -> operate(helper, operation, target));
        } catch (RuntimeException refused) {
            // A closed executor can occur during unregistration; fall back to synchronous
            // delivery.
            failures.incrementAndGet();
            operate(helper, operation, target);
        }
    }

    /** Execute on the delivery thread and retain wrapper-reported counts. */
    private void operate(Object helper, Object operation, Object target) {
        try {
            perform(operation, target);
            delivered.incrementAndGet();
        } catch (ReflectiveOperationException error) {
            failure = describe(error);
            failures.incrementAndGet();
        }
    }

    /** Unwrap reflection exceptions to retain the underlying failure. */
    private static String describe(Throwable error) {
        Throwable cause = error;
        while (cause instanceof java.lang.reflect.InvocationTargetException
                && cause.getCause() != null) {
            cause = cause.getCause();
        }
        StringBuilder text = new StringBuilder(cause.getClass().getName())
                                     .append(": ")
                                     .append(cause.getMessage());
        // Include the stack trace to identify the failing layer.
        for (StackTraceElement frame : cause.getStackTrace()) {
            text.append(" <- ")
                    .append(frame.getClassName())
                    .append('.')
                    .append(frame.getMethodName())
                    .append(':')
                    .append(frame.getLineNumber());
            if (text.length() > 600)
                break;
        }
        // Keep diagnostic text within the heartbeat frame limit.
        return text.length() > 700 ? text.substring(0, 700) : text.toString();
    }

    /**
     * Resolve operate once using its erased Object parameter, rather than the listener interface
     * type.
     */
    private void perform(Object operation, Object target) throws ReflectiveOperationException {
        Method resolved = operate;
        if (resolved == null) {
            resolved = operationType.getMethod("operate", Object.class);
            resolved.setAccessible(true);
            operate = resolved;
        }
        resolved.invoke(operation, target);
    }

    private Executor executor(Object registration) {
        try {
            Object value = executorField().get(registration);
            return value instanceof Executor available ? available : null;
        } catch (ReflectiveOperationException | RuntimeException error) {
            return null;
        }
    }

    private Field executorField() throws NoSuchFieldException {
        reflect();
        return executor;
    }

    /**
     * Cache the executor field from ListenerRegistration. The operation receives the listener
     * without modifying the platform's listener field.
     */
    private void reflect() throws NoSuchFieldException {
        if (reflected)
            return;
        for (Class<?> type = listenerType; type != null; type = type.getSuperclass()) {
            if (executor == null) {
                try {
                    executor = type.getDeclaredField("mExecutor");
                    executor.setAccessible(true);
                } catch (NoSuchFieldException ignored) {
                }
            }
        }
        reflected = true;
    }

    /**
     * Comma-separated registration, dispatch, eligibility, attempt, delivery, failure and wrapper
     * counters followed by failure details.
     */
    List<String> counters() {
        int known;
        synchronized (registrations) {
            known = registrations.size();
        }
        return List.of(String.valueOf(known), String.valueOf(dispatchCalls.get()),
                String.valueOf(needs.get()), String.valueOf(ticks.get()),
                String.valueOf(delivered.get()), String.valueOf(failures.get()),
                String.valueOf(wrapperTicks), String.valueOf(wrapperDeliveries),
                clean(failure == null ? "-" : failure),
                clean(wrapperIdentity == null ? "-" : wrapperIdentity));
    }

    /** Sanitize delimiters reserved for channel, index and field separation. */
    private static String clean(String text) {
        return text.replace(';', '/').replace(':', '=').replace(',', ' ');
    }

    private static Method optional(Class<?> type, String name, Class<?>... args) {
        try {
            return method(type, name, args);
        } catch (NoSuchMethodException missing) {
            return null;
        }
    }

    private static Method method(Class<?> type, String name, Class<?>... args)
            throws NoSuchMethodException {
        for (Class<?> current = type; current != null; current = current.getSuperclass()) {
            try {
                Method method = current.getDeclaredMethod(name, args);
                method.setAccessible(true);
                return method;
            } catch (NoSuchMethodException ignored) {
            }
        }
        throw new NoSuchMethodException(type.getName() + "." + name);
    }
}
