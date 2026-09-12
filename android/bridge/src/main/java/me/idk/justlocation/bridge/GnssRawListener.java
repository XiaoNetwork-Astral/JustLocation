package me.idk.justlocation.bridge;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.List;
import java.util.function.Function;
import java.util.function.LongSupplier;
import java.util.function.Supplier;

/**
 * 原始 GNSS 数据通道（原始测量 / 导航电文）的监听包装。
 *
 * <p>与 {@link GnssListener} 的差别：这两条接口各自只有一个数据回调
 * （`onGnssMeasurementsReceived` / `onGnssNavigationMessageReceived`），没有 started/stopped/firstFix
 * 那一套生命周期，所以不需要 NMEA 那样的"挡下真实报文、停止后原样补发"。
 *
 * <p>`onStatusChanged` **原样转发**：它报的是设备的能力状态（支不支持原始测量、定位开没开），
 * 那是真实属性，不该由我们编。
 *
 * <p>取数只发生在平台的活跃投递里（见 {@link GnssDispatcher}），所以权限、AppOps、
 * 注册与 binder 死亡都还是系统自己那套。
 */
final class GnssRawListener implements InvocationHandler, GnssTick {
    /**
     * `enabled` 与卫星状态通道共用后台的 `gnss_enabled` 开关。
     *
     * <p>为什么不另开两个开关：对使用者来说"开了 GNSS 模拟"就该三条出口一起接管；
     * 分成三个开关只会让配置与面板复杂化，而现在连面板都还没有。
     */
    record Output(SessionSnapshot scope, GnssFrame frame, boolean enabled) {}

    private final Object delegate, proxy;
    private final String packageName;
    private final Class<?> type;
    private final Supplier<Output> output;
    private final LongSupplier clock;
    private final Method event;
    private final Function<GnssFrame, List<Object>> build;
    private final java.util.concurrent.atomic.AtomicLong ticks = new java.util.concurrent.atomic.AtomicLong();
    private final java.util.concurrent.atomic.AtomicLong deliveries = new java.util.concurrent.atomic.AtomicLong();

    GnssRawListener(Class<?> type, Object delegate, String packageName, Supplier<Output> output,
                    LongSupplier clock, String eventName, Function<GnssFrame, List<Object>> build) {
        this.delegate = delegate; this.packageName = packageName; this.output = output;
        this.type = type; this.clock = clock; this.build = build;
        event = find(type, eventName);
        if (event == null) throw new IllegalArgumentException(type.getName() + " lacks " + eventName);
        proxy = Proxy.newProxyInstance(type.getClassLoader(), new Class<?>[]{type}, this);
    }

    Object proxy() { return proxy; }

    @Override public synchronized boolean needsTick() {
        Output value = current();
        return value != null && value.enabled();
    }

    private Output current() {
        Output value = output.get();
        return value != null && value.scope.appliesTo(packageName, clock.getAsLong()) ? value : null;
    }

    @Override public synchronized Object invoke(Object proxy, Method method, Object[] args) throws Throwable {
        if (method.getDeclaringClass() == Object.class) return switch (method.getName()) {
            case "equals" -> proxy == args[0];
            case "hashCode" -> System.identityHashCode(proxy);
            default -> "JustLocation GNSS raw callback";
        };
        // 模拟期间挡下真实数据：放过去的话，应用会看到真假两套读数交替出现。
        // 这两条通道不做补发——原始测量与导航电文是"当下这一刻的天空"，事后补发没有意义。
        Output value = current();
        if (method.equals(event) && value != null && value.enabled()) return null;
        return call(method, args);
    }

    @Override public synchronized void tick() throws Throwable {
        ticks.incrementAndGet();
        Output value = current();
        if (value == null || !value.enabled()) return;
        for (Object item : build.apply(value.frame)) {
            // 逐条重查：投递过程中可能刚好停止、过期或换了快照。
            if (current() != value || !value.enabled()) return;
            call(event, new Object[]{item});
            deliveries.incrementAndGet();
        }
    }

    /** 诊断用：tick 被进入的次数、真正调用到应用监听器的次数。 */
    @Override public long ticks() { return ticks.get(); }
    @Override public long deliveries() { return deliveries.get(); }

    /**
     * 诊断用：进入投递时看到的接收方与事件方法属于哪个接口。
     *
     * <p>为什么必须记下来：真机上导航电文那条报
     * `Expected receiver of type GnssMeasurement but got GnssNavigationMessage`——
     * 这句话只说明"有人被喂了导航电文却按测量接收"，**没说接收方到底是谁**。
     * 把"接收方的实现类 + 事件方法的声明接口"记下来，才能分清是**我们发错了对象**，
     * 还是**接收端自己把两条通道搞混了**。
     */
    @Override public String description() {
        Method value = event;
        StringBuilder interfaces = new StringBuilder();
        for (Class<?> implemented : delegate.getClass().getInterfaces()) {
            if (interfaces.length() > 0) interfaces.append('+');
            interfaces.append(implemented.getName());
        }
        return "recv=" + delegate.getClass().getName() + " implements=" + interfaces
                + " iface=" + type.getName()
                + " event=" + (value == null ? "none" : value.getDeclaringClass().getName() + '#' + value.getName());
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
