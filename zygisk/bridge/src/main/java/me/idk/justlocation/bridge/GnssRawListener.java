package me.idk.justlocation.bridge;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.List;
import java.util.function.Function;
import java.util.function.LongSupplier;
import java.util.function.Supplier;

/** Proxy raw measurement and navigation listeners using the platform callback interfaces. */
final class GnssRawListener implements InvocationHandler, GnssTick {
    /** Raw channels share the backend gnss_enabled flag with satellite status. */
    record Output(SessionSnapshot scope, GnssFrame frame, boolean enabled) {}

    private final Object delegate, proxy;
    private final String packageName;
    private final Class<?> type;
    private final Supplier<Output> output;
    private final LongSupplier clock;
    private final Method event;
    private final Function<GnssFrame, List<Object>> build;
    private final java.util.concurrent.atomic.AtomicLong ticks =
            new java.util.concurrent.atomic.AtomicLong();
    private final java.util.concurrent.atomic.AtomicLong deliveries =
            new java.util.concurrent.atomic.AtomicLong();

    GnssRawListener(Class<?> type, Object delegate, String packageName, Supplier<Output> output,
            LongSupplier clock, String eventName, Function<GnssFrame, List<Object>> build) {
        this.delegate = delegate;
        this.packageName = packageName;
        this.output = output;
        this.type = type;
        this.clock = clock;
        this.build = build;
        event = find(type, eventName);
        if (event == null)
            throw new IllegalArgumentException(type.getName() + " lacks " + eventName);
        proxy = Proxy.newProxyInstance(type.getClassLoader(), new Class<?>[] {type}, this);
    }

    Object proxy() {
        return proxy;
    }

    @Override
    public synchronized boolean needsTick() {
        Output value = current();
        return value != null && value.enabled();
    }

    private Output current() {
        Output value = output.get();
        return value != null && value.scope.appliesTo(packageName, clock.getAsLong()) ? value
                                                                                      : null;
    }

    @Override
    public synchronized Object invoke(Object proxy, Method method, Object[] args) throws Throwable {
        if (method.getDeclaringClass() == Object.class)
            return switch (method.getName()) {
                case "equals" -> proxy == args[0];
                case "hashCode" -> System.identityHashCode(proxy);
                default -> "JustLocation GNSS raw callback";
            };
        // Suppress real events during simulation. Raw measurements and navigation messages are
        // time-sensitive and are not replayed later.
        Output value = current();
        if (method.equals(event) && value != null && value.enabled())
            return null;
        return call(method, args);
    }

    @Override
    public synchronized void tick() throws Throwable {
        ticks.incrementAndGet();
        Output value = current();
        if (value == null || !value.enabled())
            return;
        for (Object item : build.apply(value.frame)) {
            // Recheck before each event in case the snapshot stops, expires or changes.
            if (current() != value || !value.enabled())
                return;
            call(event, new Object[] {item});
            deliveries.incrementAndGet();
        }
    }

    /** Count tick entries and actual listener calls. */
    @Override
    public long ticks() {
        return ticks.get();
    }
    @Override
    public long deliveries() {
        return deliveries.get();
    }

    /** Report the receiver class and event interface to identify mismatched callback types. */
    @Override
    public String description() {
        Method value = event;
        StringBuilder interfaces = new StringBuilder();
        for (Class<?> implemented : delegate.getClass().getInterfaces()) {
            if (interfaces.length() > 0)
                interfaces.append('+');
            interfaces.append(implemented.getName());
        }
        return "recv=" + delegate.getClass().getName() + " implements=" + interfaces
                + " iface=" + type.getName() + " event="
                + (value == null ? "none"
                                 : value.getDeclaringClass().getName() + '#' + value.getName());
    }

    private Object call(Method method, Object[] args) throws Throwable {
        try {
            return method.invoke(delegate, args);
        } catch (InvocationTargetException error) {
            throw error.getCause();
        }
    }

    private static Method find(Class<?> type, String name) {
        for (Method method : type.getMethods())
            if (method.getName().equals(name))
                return method;
        return null;
    }
}
