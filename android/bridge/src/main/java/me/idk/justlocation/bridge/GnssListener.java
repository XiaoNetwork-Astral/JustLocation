package me.idk.justlocation.bridge;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.function.Function;
import java.util.function.LongSupplier;
import java.util.function.Supplier;

/** Wraps a service-side callback while retaining the client's binder and platform registration. */
final class GnssListener implements InvocationHandler {
    record Output(SessionSnapshot scope, GnssFrame frame) {}
    private final Object delegate, proxy;
    private final String packageName;
    private final Supplier<Output> output;
    private final LongSupplier clock;
    private final Function<GnssFrame, Object> status;
    private final Method started, stopped, firstFix, svStatus, nmea;
    private boolean simulated, navigating;
    private Integer realFirstFix;

    GnssListener(Class<?> type, Object delegate, String packageName, Supplier<Output> output,
                 LongSupplier clock, Function<GnssFrame, Object> status) {
        this.delegate = delegate; this.packageName = packageName; this.output = output;
        this.clock = clock; this.status = status;
        started = find(type, "onGnssStarted"); stopped = find(type, "onGnssStopped");
        firstFix = find(type, "onFirstFix"); svStatus = find(type, "onSvStatusChanged"); nmea = find(type, "onNmeaReceived");
        proxy = Proxy.newProxyInstance(type.getClassLoader(), new Class<?>[]{type}, this);
    }
    Object proxy() { return proxy; }
    synchronized boolean needsTick() { return simulated || current() != null; }

    private Output current() {
        Output value = output.get();
        return value != null && value.scope.appliesTo(packageName, clock.getAsLong()) ? value : null;
    }

    @Override public synchronized Object invoke(Object proxy, Method method, Object[] args) throws Throwable {
        if (method.getDeclaringClass() == Object.class) return switch (method.getName()) {
            case "equals" -> proxy == args[0];
            case "hashCode" -> System.identityHashCode(proxy);
            default -> "JustLocation GNSS callback";
        };
        String name = method.getName();
        boolean event = name.equals("onGnssStarted") || name.equals("onGnssStopped") || name.equals("onFirstFix")
                || name.equals("onSvStatusChanged") || name.equals("onNmeaReceived");
        // asBinder is always the original, so unregister and binder death use the real identity.
        if (!event) return call(method, args);
        if (current() == null) {
            restore();
            trackReal(name, args);
            return call(method, args);
        }
        trackReal(name, args);
        // Periodic delivery owns the simulated stream. Do not mix in HAL events.
        return null;
    }

    private void trackReal(String name, Object[] args) {
        if (name.equals("onGnssStarted")) { navigating = true; realFirstFix = null; }
        else if (name.equals("onGnssStopped")) { navigating = false; realFirstFix = null; }
        else if (name.equals("onFirstFix")) realFirstFix = (Integer) args[0];
    }

    /** Invoked only by the platform's active-registration delivery operation. */
    synchronized void tick() throws Throwable {
        Output value = current();
        if (value == null) { restore(); return; }
        if (nmea != null) {
            for (String sentence : value.frame.nmea()) {
                // Recheck before each call, including a stop received while queued for delivery.
                if (current() != value) return;
                call(nmea, new Object[]{value.frame.timestampMs, sentence});
            }
        } else {
            if (!simulated) {
                call(started, null); call(firstFix, new Object[]{0}); simulated = true;
            }
            if (current() == value) call(svStatus, new Object[]{status.apply(value.frame)});
        }
    }

    private void restore() throws Throwable {
        if (!simulated) return;
        simulated = false;
        call(stopped, null);
        if (navigating) {
            call(started, null);
            if (realFirstFix != null) call(firstFix, new Object[]{realFirstFix});
        }
    }
    private Object call(Method method, Object[] args) throws Throwable {
        try { return method.invoke(delegate, args); }
        catch (InvocationTargetException error) { throw error.getCause(); }
    }
    private static Method find(Class<?> type, String name) {
        for (Method method : type.getMethods()) if (method.getName().equals(name)) return method;
        return null;
    }
}
