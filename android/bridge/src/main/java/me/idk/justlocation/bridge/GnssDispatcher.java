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
 * Per-provider adapter. The platform retains active, permission, executor and binder-death rules.
 *
 * <p>投递有两条路，由构造参数的 {@code selfDriven} 选择：
 * <ul>
 *   <li>{@code false}：沿用平台自己的投递，我们只是把监听器换成代理。卫星状态与 NMEA
 *       走这条——它们本来就由真实 HAL 回调驱动。做法是把我们的 {@link ListenerOperation}
 *       工厂交给平台的 {@code deliverToListeners(Function)}，由它套到每个活跃注册上执行。</li>
 *   <li>{@code true}：**自己驱动投递**。原始测量与导航电文走这条。原因是"挂钩就绪"不等于
 *       "平台会来叫我们"：<b>导航电文完全依赖 HAL 回调 {@code onReportNavigationMessage}
 *       触发投递，而这台设备的 HAL 是否上报导航电文采集能力，我们无法保证</b>；一旦它不上报，
 *       平台一次都不会调用投递，应用就一条也收不到（真机现象：钩子被走到、注册也包住了、
 *       应用侧收到 0 条）。既然这两条出口的数据全部由我们合成，投递也应当由我们发起。</li>
 * </ul>
 *
 * <p>自己驱动时仍然复用平台的对象与规则，不自己造一套：
 * 注册对象是平台建的，监听器是注册里存的那个代理，{@code operate} 里做的是平台同款的
 * AppOps 检查（{@code noteOpNoThrow(1, identity)}，1 就是 {@code OP_FINE_LOCATION}），
 * 投递线程优先用注册自带的 {@code mExecutor}（没有才退化成同步调用）。
 * 作用范围与快照新鲜度不在这一层——它们由 {@link GnssTick#needsTick()} 判断。
 */
final class GnssDispatcher {
    private final Set<Object> providers = Collections.newSetFromMap(new WeakHashMap<>());
    /**
     * 自己收集到的注册：**代理**与它的身份。
     *
     * <p>为什么不复用平台的 {@code mRegistrations}：那张表按 key 归并，一个 key 只留一个
     * "合并注册"；更关键的是 {@code deliverToListeners} 只喂**活跃**注册，而"导航电文送不到"
     * 的教训正是——**不能把"应用能不能收到"押在平台的活跃位与 HAL 回调上**。
     * 我们自己盯住 {@code addListener} 的每一次调用，注册过就一定投递得到。
     */
    private final List<Entry> registrations = new ArrayList<>();
    private final Class<?> listenerType;
    private final Method deliver, getIdentity, packageName, noteOp;
    private final Field appOps;
    private Field executor, listener;
    private Method operate;
    private boolean reflected;
    /**
     * 把"平台的监听器"包成我们的代理。**做成工厂而不是写死 `GnssListener`**：
     * 卫星状态/NMEA 与原始测量/导航电文的接口形状不同，但投递机制完全一样，
     * 所以这里只认 {@link GnssTick}，谁包进来由调用方决定。
     */
    private final BiFunction<Object, String, Object> wrapper;
    private final Class<?> operationType;
    private final boolean selfDriven;

    /**
     * 注册表里的一条：**只持弱引用**，外加它的身份。
     *
     * <p>为什么不直接强引用那个代理：应用注销或进程退出之后，条目会永远留着，
     * 每一轮 1 秒心跳都对着一个已经死掉的 binder 投一次，失败计数无限涨
     * （真机上见过 20 条陈旧注册、4000 多次 `DeadObjectException`）。弱引用一到
     * 应用放手就自然判活，比"投失败再删"干净。
     */
    private record Entry(java.lang.ref.WeakReference<Object> target, Object identity) {}

    private final AtomicInteger applied = new AtomicInteger();
    private final AtomicInteger needs = new AtomicInteger();
    private final AtomicInteger ticks = new AtomicInteger();
    private final AtomicInteger delivered = new AtomicInteger();
    private final AtomicInteger failures = new AtomicInteger();
    private final AtomicInteger dispatchCalls = new AtomicInteger();
    /**
     * 最近一次投递失败的原因：`异常类名: 消息`。
     *
     * <p>为什么要把它也报出去：这一层以前**只加计数、不留原因**，于是真机上"0 次送达"
     * 只能看出失败，看不出失败在哪——而 `system_server` 的日志既写不进
     * `/data/adb/justlocation`、又会被环形缓冲冲掉。异常正文跟着状态回包走，才查得动。
     */
    private volatile String failure;
    /** 包装层自报的计数（进入投递次数 / 真正送出的条数），`operate` 每次执行后更新。 */
    private volatile long wrapperTicks = -1, wrapperDeliveries = -1;
    /** 包装层自报的接收方身份（见 {@link GnssTick#description()}）。 */
    private volatile String wrapperIdentity;

    GnssDispatcher(Class<?> provider, Class<?> listener, Class<?> registration, Class<?> identity, Class<?> operationType,
                   BiFunction<Object, String, Object> wrapper) throws Exception {
        this(provider, listener, registration, identity, operationType, wrapper, false);
    }

    GnssDispatcher(Class<?> provider, Class<?> listener, Class<?> registration, Class<?> identity, Class<?> operationType,
                   BiFunction<Object, String, Object> wrapper, boolean selfDriven) throws Exception {
        this.listenerType = listener; this.wrapper = wrapper; this.selfDriven = selfDriven;
        // 自己驱动时平台那条投递只是"额外一份"，取不到也不算这条通道装不上；
        // 平台驱动时它才是唯一的投递路径，必须拿到。
        deliver = selfDriven ? optional(provider, "deliverToListeners", Function.class)
                : method(provider, "deliverToListeners", Function.class);
        getIdentity = method(registration, "getIdentity"); packageName = identity.getMethod("getPackageName");
        appOps = provider.getDeclaredField("mAppOpsHelper"); appOps.setAccessible(true);
        noteOp = appOps.getType().getMethod("noteOpNoThrow", int.class, identity);
        this.operationType = operationType;
    }

    private Object operation(Object helper, Object identity) {
        return Proxy.newProxyInstance(operationType.getClassLoader(), new Class<?>[]{operationType}, (proxy, method, args) -> {
            if (method.getName().equals("operate")) {
                Object target = args[0];
                if (Proxy.isProxyClass(target.getClass()) && Proxy.getInvocationHandler(target) instanceof GnssTick handler) {
                    needs.incrementAndGet();
                    if (handler.needsTick()) {
                        ticks.incrementAndGet();
                        // 在投递**之前**记下接收方身份：投递失败时也要能看到它是谁。
                        if (handler.description() != null) wrapperIdentity = handler.description();
                        boolean permitted;
                        try { permitted = (boolean) noteOp.invoke(helper, 1, identity); }
                        catch (ReflectiveOperationException error) {
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
            } else if (method.getDeclaringClass() == Object.class) return switch (method.getName()) {
                case "equals" -> proxy == args[0]; case "hashCode" -> System.identityHashCode(proxy);
                default -> "JustLocation GNSS delivery";
            };
            // Android 15 ListenerOperation lifecycle defaults are all empty, void methods.
            return null;
        });
    }

    Object wrap(Object identity, Object listener) throws Exception {
        Object wrapped = wrapper.apply(listener, (String) packageName.invoke(identity));
        synchronized (registrations) { registrations.add(new Entry(new java.lang.ref.WeakReference<>(wrapped), identity)); }
        return wrapped;
    }

    void track(Object provider) { synchronized (providers) { providers.add(provider); } }

    /**
     * 投递一轮。
     *
     * <p>平台驱动（卫星状态、NMEA）：让平台的 `deliverToListeners(Function)` 把我们的
     * 操作套到每个活跃注册上——权限、AppOps、执行器、binder 死亡都仍是系统自己那套。
     *
     * <p>自己驱动（原始测量、导航电文）：直接投给我们自己收集到的注册。**不依赖平台的活跃位，
     * 也不依赖 HAL 回调**；AppOps 与注册自带的执行器照用，所以"该不该给这个应用"仍由平台的
     * 规则决定（AppOps 在 `operate` 里查，作用范围与开关在 `needsTick` 里查）。
     */
    void dispatch() throws Exception {
        dispatchCalls.incrementAndGet();
        ArrayList<Object> snapshot;
        synchronized (providers) { snapshot = new ArrayList<>(providers); }
        if (selfDriven) {
            for (Object provider : snapshot) drive(appOps.get(provider));
            return;
        }
        if (deliver == null) return;
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

    /** 自己驱动投递：不依赖平台是否来叫我们、也不依赖它有没有把注册标成活跃。 */
    private void drive(Object helper) throws Exception {
        ArrayList<Entry> entries;
        synchronized (registrations) { entries = new ArrayList<>(registrations); }
        int ready = 0;
        for (Entry entry : entries) {
            Object target = entry.target().get();
            // 代理已经被应用丢掉（注销或进程退出）：这一条再投也是白投，而且会一直堆在这里。
            // **按弱引用判活**，不要看投递是否抛异常——那个 `DeadObjectException` 是真的，
            // 但它只是"人去楼空"的后果，拿它当判据会让失败计数无限涨。
            if (target == null) {
                synchronized (registrations) { registrations.remove(entry); }
                continue;
            }
            ready++;
            delivers(helper, target, entry.identity());
        }
        if (ready > 0) applied.incrementAndGet();
    }

    /** 一个注册的一次投递：优先在注册自己的执行器上跑（平台就是这么做的），拿不到就同步跑。 */
    private void delivers(Object helper, Object target, Object identity) throws Exception {
        Object operation = operation(helper, identity);
        Executor executor;
        try { executor = executor(identity); }
        catch (RuntimeException error) { executor = null; }
        if (executor == null) { operate(helper, operation, target); return; }
        try { executor.execute(() -> operate(helper, operation, target)); }
        catch (RuntimeException refused) {
            // 执行器已经关了（应用正好注销）不是异常路径，退回同步执行。
            failures.incrementAndGet();
            operate(helper, operation, target);
        }
    }

    /** 在投递线程上执行；包装类会记账，所以成功一次加一。 */
    private void operate(Object helper, Object operation, Object target) {
        try {
            perform(operation, target);
            delivered.incrementAndGet();
        } catch (ReflectiveOperationException error) {
            failure = describe(error);
            failures.incrementAndGet();
        }
    }

    /** 异常可能被反射包了一层，剥到最里面那层再说，否则全是 InvocationTargetException。 */
    private static String describe(Throwable error) {
        Throwable cause = error;
        while (cause instanceof java.lang.reflect.InvocationTargetException && cause.getCause() != null) {
            cause = cause.getCause();
        }
        StringBuilder text = new StringBuilder(cause.getClass().getName()).append(": ").append(cause.getMessage());
        // 连调用栈一起报：这条异常是"哪一层的哪一句"扔的，只有栈说得清。
        for (StackTraceElement frame : cause.getStackTrace()) {
            text.append(" <- ").append(frame.getClassName()).append('.').append(frame.getMethodName())
                    .append(':').append(frame.getLineNumber());
            if (text.length() > 600) break;
        }
        // 计数串要放进一次 socket 帧里，别让它无限长。
        return text.length() > 700 ? text.substring(0, 700) : text.toString();
    }

    /**
     * `operate` 的 Method 只解析一次：每小时 3600 轮投递不值得每次做方法查找。
     *
     * <p>**查的是擦除后的参数类型 `Object`，不是监听器接口**：`ListenerOperation<TListener>`
     * 编译出来是 `operate(Object)`（真机 `services.jar` 里核过），按接口类型去 `getMethod`
     * 只会抛 `NoSuchMethodException`——而那种失败在外面看起来只是"投递没发生"。
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
        } catch (ReflectiveOperationException | RuntimeException error) { return null; }
    }

    private Field executorField() throws NoSuchFieldException { reflect(); return executor; }

    /**
     * 执行器字段在 `ListenerRegistration` 基类上，一次反射反复用。
     *
     * <p>**不找 `mListener`**：平台的注册对象是它自己造的，我们不去改它的字段——
     * 投递走 {@link #operation} 造的操作（它自己会从参数里拿到监听器），
     * 少碰一处平台的内部状态就少一处"看起来成功、实际没生效"。
     */
    private void reflect() throws NoSuchFieldException {
        if (reflected) return;
        for (Class<?> type = listenerType; type != null; type = type.getSuperclass()) {
            if (executor == null) {
                try { executor = type.getDeclaredField("mExecutor"); executor.setAccessible(true); }
                catch (NoSuchFieldException ignored) { }
            }
        }
        reflected = true;
    }

    /**
     * 诊断用：注册数 / dispatch 调用次数 / 该投次数 / 真投次数 / 送达数 / 失败数 /
     * 包装进入次数 / 包装送出条数 / 失败原因。**逗号分隔的每一项都不含逗号**，
     * 因为 `;` 与 `:` 已经用来分隔通道与下标了。
     */
    List<String> counters() {
        int known;
        synchronized (registrations) { known = registrations.size(); }
        return List.of(String.valueOf(known), String.valueOf(dispatchCalls.get()),
                String.valueOf(needs.get()), String.valueOf(ticks.get()),
                String.valueOf(delivered.get()), String.valueOf(failures.get()),
                String.valueOf(wrapperTicks), String.valueOf(wrapperDeliveries),
                clean(failure == null ? "-" : failure),
                clean(wrapperIdentity == null ? "-" : wrapperIdentity));
    }

    /** 计数串用 `;` 分通道、`:` 分下标、`,` 分项，所以正文里的这三个字符必须换掉。 */
    private static String clean(String text) {
        return text.replace(';', '/').replace(':', '=').replace(',', ' ');
    }

    private static Method optional(Class<?> type, String name, Class<?>... args) {
        try { return method(type, name, args); }
        catch (NoSuchMethodException missing) { return null; }
    }

    private static Method method(Class<?> type, String name, Class<?>... args) throws NoSuchMethodException {
        for (Class<?> current = type; current != null; current = current.getSuperclass()) {
            try { Method method = current.getDeclaredMethod(name, args); method.setAccessible(true); return method; }
            catch (NoSuchMethodException ignored) { }
        }
        throw new NoSuchMethodException(type.getName() + "." + name);
    }
}
