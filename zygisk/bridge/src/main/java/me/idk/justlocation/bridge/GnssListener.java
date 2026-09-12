package me.idk.justlocation.bridge;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayDeque;
import java.util.function.Function;
import java.util.function.LongSupplier;
import java.util.function.Supplier;

/**
 * Wraps a service-side callback while retaining the client's binder and platform registration.
 *
 * <p>两个卫星通道各自受后台的开关控制：`GNSS 状态` 决定是否投递合成卫星状态，
 * `NMEA 报文` 决定是否拦截 NMEA。它们都只在"定位模拟进行中且本应用在作用范围内"时生效；
 * 任一条件不满足就原样转发系统回调——也就是"未启用等于系统原样"。
 */
final class GnssListener implements InvocationHandler, GnssTick {
    /** enabledGnss / enabledNmea 来自后台配置，默认关闭。 */
    record Output(SessionSnapshot scope, GnssFrame frame, boolean enabledGnss, boolean enabledNmea) {}
    /** 模拟期间到达的真实报文，停止后按原样补发，避免应用看到报文凭空消失。 */
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
        this.delegate = delegate; this.packageName = packageName; this.output = output;
        this.clock = clock; this.status = status;
        started = find(type, "onGnssStarted"); stopped = find(type, "onGnssStopped");
        firstFix = find(type, "onFirstFix"); svStatus = find(type, "onSvStatusChanged"); nmea = find(type, "onNmeaReceived");
        nmeaChannel = nmea != null && svStatus == null;
        proxy = Proxy.newProxyInstance(type.getClassLoader(), new Class<?>[]{type}, this);
    }
    Object proxy() { return proxy; }

    /** 该通道当前是否由模拟接管。 */
    @Override public synchronized boolean needsTick() {
        Output value = current();
        return value != null && (nmeaChannel ? value.enabledNmea() : value.enabledGnss());
    }

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
        Output value = current();
        boolean takeOver = value != null && (nmeaChannel ? value.enabledNmea() : value.enabledGnss());
        if (!takeOver) {
            restore();
            trackReal(name, args);
            return call(method, args);
        }
        trackReal(name, args);
        // 模拟期间到达的真实报文先留着，停止时补发；真实状态事件直接丢弃，
        // 否则会与合成状态交替出现，应用侧看起来像信号在乱跳。
        if (nmeaChannel && name.equals("onNmeaReceived")) {
            heldNmea.addLast(args);
            while (heldNmea.size() > REPLAY_LIMIT) heldNmea.removeFirst();
        }
        return null;
    }

    private void trackReal(String name, Object[] args) {
        if (name.equals("onGnssStarted")) { navigating = true; realFirstFix = null; }
        else if (name.equals("onGnssStopped")) { navigating = false; realFirstFix = null; }
        else if (name.equals("onFirstFix")) realFirstFix = (Integer) args[0];
    }

    /** Invoked only by the platform's active-registration delivery operation. */
    @Override public synchronized void tick() throws Throwable {
        Output value = current();
        if (value == null || (nmeaChannel ? !value.enabledNmea() : !value.enabledGnss())) {
            restore();
            return;
        }
        if (nmeaChannel) {
            for (String sentence : value.frame.nmea()) {
                // Recheck before each call, including a stop received while queued for delivery.
                if (current() != value || !value.enabledNmea()) return;
                call(nmea, new Object[]{value.frame.timestampMs, sentence});
            }
        } else {
            if (!simulated) {
                call(started, null); call(firstFix, new Object[]{0}); simulated = true;
            }
            if (current() == value && value.enabledGnss()) call(svStatus, new Object[]{status.apply(value.frame)});
        }
    }

    private void restore() throws Throwable {
        if (!simulated && heldNmea.isEmpty()) return;
        if (simulated) {
            simulated = false;
            call(stopped, null);
            if (navigating) {
                call(started, null);
                if (realFirstFix != null) call(firstFix, new Object[]{realFirstFix});
            }
        }
        // 补发模拟期间被挡下的真实报文，让应用的报文流连续不断。
        while (!heldNmea.isEmpty()) call(nmea, heldNmea.removeFirst());
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
