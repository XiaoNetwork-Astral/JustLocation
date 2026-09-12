package me.idk.justlocation.bridge;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayDeque;
import java.util.function.Function;
import java.util.function.LongSupplier;
import java.util.function.Supplier;

/** Satellite and NMEA listener proxy with shared scope and snapshot expiry checks. */
final class GnssListener implements InvocationHandler, GnssTick {
    /** GNSS and NMEA flags come from backend settings and default to disabled. */
    record Output(
            SessionSnapshot scope, GnssFrame frame, boolean enabledGnss, boolean enabledNmea) {}
    /** Retain real NMEA messages during simulation for replay after it stops. */
    private static final int REPLAY_LIMIT = 32;
    private final Object delegate, proxy;
    private final String packageName;
    private final Supplier<Output> output;
    private final LongSupplier clock;
    private final Function<GnssFrame, Object> status;
    private final Method started, stopped, firstFix, svStatus, nmea;
    private final boolean nmeaChannel;
    private final ArrayDeque<Object[]> heldNmea = new ArrayDeque<>();
    private boolean simulated, navigating;
    private Integer realFirstFix;

    GnssListener(Class<?> type, Object delegate, String packageName, Supplier<Output> output,
            LongSupplier clock, Function<GnssFrame, Object> status) {
        this.delegate = delegate;
        this.packageName = packageName;
        this.output = output;
        this.clock = clock;
        this.status = status;
        started = find(type, "onGnssStarted");
        stopped = find(type, "onGnssStopped");
        firstFix = find(type, "onFirstFix");
        svStatus = find(type, "onSvStatusChanged");
        nmea = find(type, "onNmeaReceived");
        nmeaChannel = nmea != null && svStatus == null;
        proxy = Proxy.newProxyInstance(type.getClassLoader(), new Class<?>[] {type}, this);
    }
    Object proxy() {
        return proxy;
    }

    /** Whether this channel currently supplies simulated output. */
    @Override
    public synchronized boolean needsTick() {
        Output value = current();
        return value != null && (nmeaChannel ? value.enabledNmea() : value.enabledGnss());
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
                default -> "JustLocation GNSS callback";
            };
        String name = method.getName();
        boolean event = name.equals("onGnssStarted") || name.equals("onGnssStopped")
                || name.equals("onFirstFix") || name.equals("onSvStatusChanged")
                || name.equals("onNmeaReceived");
        // asBinder is always the original, so unregister and binder death use the real identity.
        if (!event)
            return call(method, args);
        Output value = current();
        boolean takeOver =
                value != null && (nmeaChannel ? value.enabledNmea() : value.enabledGnss());
        if (!takeOver) {
            restore();
            trackReal(name, args);
            return call(method, args);
        }
        trackReal(name, args);
        // Queue real NMEA messages during simulation; suppress real status events to avoid
        // alternating real and simulated satellite state.
        if (nmeaChannel && name.equals("onNmeaReceived")) {
            heldNmea.addLast(args);
            while (heldNmea.size() > REPLAY_LIMIT)
                heldNmea.removeFirst();
        }
        return null;
    }

    private void trackReal(String name, Object[] args) {
        if (name.equals("onGnssStarted")) {
            navigating = true;
            realFirstFix = null;
        } else if (name.equals("onGnssStopped")) {
            navigating = false;
            realFirstFix = null;
        } else if (name.equals("onFirstFix"))
            realFirstFix = (Integer) args[0];
    }

    /** Invoked only by the platform's active-registration delivery operation. */
    @Override
    public synchronized void tick() throws Throwable {
        Output value = current();
        if (value == null || (nmeaChannel ? !value.enabledNmea() : !value.enabledGnss())) {
            restore();
            return;
        }
        if (nmeaChannel) {
            for (String sentence : value.frame.nmea()) {
                // Recheck before each call, including a stop received while queued for delivery.
                if (current() != value || !value.enabledNmea())
                    return;
                call(nmea, new Object[] {value.frame.timestampMs, sentence});
            }
        } else {
            if (!simulated) {
                call(started, null);
                call(firstFix, new Object[] {0});
                simulated = true;
            }
            if (current() == value && value.enabledGnss())
                call(svStatus, new Object[] {status.apply(value.frame)});
        }
    }

    private void restore() throws Throwable {
        if (!simulated && heldNmea.isEmpty())
            return;
        if (simulated) {
            simulated = false;
            call(stopped, null);
            if (navigating) {
                call(started, null);
                if (realFirstFix != null)
                    call(firstFix, new Object[] {realFirstFix});
            }
        }
        // Replay queued real messages when simulation stops.
        while (!heldNmea.isEmpty())
            call(nmea, heldNmea.removeFirst());
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
