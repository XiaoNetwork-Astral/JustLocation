package me.idk.justlocation.bridge;

import android.location.GnssStatus;
import android.os.SystemClock;
import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * 合成的原始 GNSS 读数：原始测量事件与导航电文。
 *
 * <p>与卫星表对齐——同一批 SVID、同一个 C/N0、同一套跟踪状态，所以应用从"卫星状态"和
 * "原始测量"两条路看到的是同一片天空。
 *
 * <p><b>边界（必须说清）</b>：这**不是射频观测**，也**不保证由这些原始数据解算出来的位置等于
 * 模拟位置**。我们没有播真实的广播星历与轨道模型，卫星位置无从算起，伪距就只能是"量级自洽"
 * 的合成值；导航电文也只保证结构成立（类型、SVID、子帧号、长度），**没有实现 IS-GPS-200 的
 * 奇偶校验与星历内容**。要做到真正的解算一致得把星历、轨道递推和奇偶编码都做出来，
 * 那是另一个量级的工程。这里的目标是：读原始数据的应用能看到一片自洽、可信、与卫星状态对得上的天空。
 *
 * <p><b>为什么全用反射</b>：这些入口在 SDK 的 `android.jar` 桩里是**包内可见**的
 * （`javap -p` 显示 `GnssClock()` 没有 `public`），但真机上运行时是公开的——已在设备上直接打印过
 * 构造器与方法（`build/location-investigation/GnssApiProbe.java`）。编译期看不见、运行时存在，
 * 正是 `WifiOutput` 用过的那套办法。任何一个入口缺失，{@link #usable()} 就是 false，
 * 两条原始通道整体不接管，绝不产出半成品对象。
 */
final class GnssRawOutput {
    private static final String CLOCK_TYPE = "android.location.GnssClock";
    private static final String MEASUREMENT_TYPE = "android.location.GnssMeasurement";
    private static final String EVENT_TYPE = "android.location.GnssMeasurementsEvent";
    private static final String MESSAGE_TYPE = "android.location.GnssNavigationMessage";

    /** 当前 GPS-UTC 闰秒。 */
    private static final int LEAP_SECONDS = 18;
    /** GPS L1 载频。 */
    private static final float L1_HZ = 1575.42e6f;
    /** 一周的纳秒数：接收机时间必须落在这一周之内。 */
    private static final long WEEK_NANOS = 604_800_000_000_000L;
    /** 信号从两万多公里高的卫星走到地面大约要这么久。 */
    private static final long TRAVEL_NANOS = 70_000_000L;
    /** 一个 L1 C/A 子帧的载荷字节数（10 个字）。 */
    private static final int SUBFRAME_BYTES = 40;

    private static final AtomicInteger DISCONTINUITIES = new AtomicInteger();
    private static final AtomicInteger SUBFRAME = new AtomicInteger();

    private static final Map<String, Method> SETTERS = new HashMap<>();
    private static final List<String> MISSING = new ArrayList<>();
    private static final Map<String, Integer> CONSTANTS = new HashMap<>();

    private static final Class<?> CLOCK = load(CLOCK_TYPE);
    private static final Class<?> MEASUREMENT = load(MEASUREMENT_TYPE);
    private static final Class<?> EVENT = load(EVENT_TYPE);
    private static final Class<?> MESSAGE = load(MESSAGE_TYPE);
    private static final Class<?> BUILDER = load(EVENT_TYPE + "$Builder");

    private static final Constructor<?> NEW_CLOCK = constructor(CLOCK);
    private static final Constructor<?> NEW_MEASUREMENT = constructor(MEASUREMENT);
    private static final Constructor<?> NEW_MESSAGE = constructor(MESSAGE);
    private static final Constructor<?> NEW_EVENT_BUILDER = constructor(BUILDER);

    private static final Method EVENT_BUILD = optional(BUILDER, "build");
    private static final Method EVENT_SET_CLOCK = optional(BUILDER, "setClock", CLOCK);
    private static final Method EVENT_SET_MEASUREMENTS = optional(BUILDER, "setMeasurements", java.util.Collection.class);
    private static final Method EVENT_SET_FULL_TRACKING = optional(BUILDER, "setIsFullTracking", boolean.class);

    /** 一次赋值：setter 名 + 参数类型。表驱动是为了不在代码里堆三十行 invoke。 */
    private record Assignment(String name, Class<?> type) {}

    private static final Assignment[] CLOCK_FIELDS = {
            new Assignment("setTimeNanos", long.class),
            new Assignment("setTimeUncertaintyNanos", double.class),
            new Assignment("setFullBiasNanos", long.class),
            new Assignment("setBiasNanos", double.class),
            new Assignment("setBiasUncertaintyNanos", double.class),
            new Assignment("setDriftNanosPerSecond", double.class),
            new Assignment("setDriftUncertaintyNanosPerSecond", double.class),
            new Assignment("setElapsedRealtimeNanos", long.class),
            new Assignment("setElapsedRealtimeUncertaintyNanos", double.class),
            new Assignment("setLeapSecond", int.class),
            new Assignment("setHardwareClockDiscontinuityCount", int.class),
    };
    private static final Assignment[] MEASUREMENT_FIELDS = {
            new Assignment("setSvid", int.class),
            new Assignment("setConstellationType", int.class),
            new Assignment("setTimeOffsetNanos", double.class),
            new Assignment("setState", int.class),
            new Assignment("setReceivedSvTimeNanos", long.class),
            new Assignment("setReceivedSvTimeUncertaintyNanos", long.class),
            new Assignment("setCn0DbHz", double.class),
            new Assignment("setBasebandCn0DbHz", double.class),
            new Assignment("setPseudorangeRateMetersPerSecond", double.class),
            new Assignment("setPseudorangeRateUncertaintyMetersPerSecond", double.class),
            new Assignment("setCarrierFrequencyHz", float.class),
            new Assignment("setMultipathIndicator", int.class),
            new Assignment("setSnrInDb", double.class),
            new Assignment("setAutomaticGainControlLevelInDb", double.class),
    };
    private static final Assignment[] MESSAGE_FIELDS = {
            new Assignment("setType", int.class),
            new Assignment("setSvid", int.class),
            new Assignment("setMessageId", int.class),
            new Assignment("setSubmessageId", int.class),
            new Assignment("setStatus", int.class),
            new Assignment("setData", byte[].class),
    };
    private static final String[] MEASUREMENT_CONSTANTS = {
            "STATE_CODE_LOCK", "STATE_SYMBOL_SYNC", "STATE_SUBFRAME_SYNC", "STATE_TOW_DECODED",
            "STATE_MSEC_AMBIGUOUS", "MULTIPATH_INDICATOR_NOT_DETECTED",
    };
    private static final String[] MESSAGE_CONSTANTS = {"TYPE_GPS_L1CA", "STATUS_PARITY_PASSED"};

    static {
        // 把用到的 setter 与常量都在这里过一遍，usable() 才知道缺没缺。
        for (Assignment assignment : CLOCK_FIELDS) setter(CLOCK, assignment.name(), assignment.type());
        for (Assignment assignment : MEASUREMENT_FIELDS) setter(MEASUREMENT, assignment.name(), assignment.type());
        for (Assignment assignment : MESSAGE_FIELDS) setter(MESSAGE, assignment.name(), assignment.type());
        for (String name : MEASUREMENT_CONSTANTS) constant(MEASUREMENT, name);
        for (String name : MESSAGE_CONSTANTS) constant(MESSAGE, name);
    }

    /** 两条原始通道必须的入口是否都在。缺任何一个就整体不接管。 */
    static boolean usable() {
        return MISSING.isEmpty() && NEW_CLOCK != null && NEW_MEASUREMENT != null && NEW_MESSAGE != null
                && NEW_EVENT_BUILDER != null && EVENT_BUILD != null && EVENT_SET_CLOCK != null
                && EVENT_SET_MEASUREMENTS != null;
    }

    /** 缺失清单，供日志说明"为什么没接管"。 */
    static String missing() { return String.join(", ", MISSING); }

    private GnssRawOutput() {}

    /** 一次原始测量：一个时钟，加上与卫星表逐颗对应的测量量。 */
    static Object measurements(GnssFrame frame) {
        long now = SystemClock.elapsedRealtimeNanos();
        long svTime = Math.floorMod(now - TRAVEL_NANOS, WEEK_NANOS);
        Object clock = build(NEW_CLOCK, CLOCK, CLOCK_FIELDS,
                now, 20.0, now - svTime, 0.0, 5.0, 12.0, 1.5, now, 1000.0, LEAP_SECONDS,
                DISCONTINUITIES.incrementAndGet());

        int state = constant(MEASUREMENT, "STATE_CODE_LOCK") | constant(MEASUREMENT, "STATE_SYMBOL_SYNC")
                | constant(MEASUREMENT, "STATE_SUBFRAME_SYNC") | constant(MEASUREMENT, "STATE_TOW_DECODED")
                | constant(MEASUREMENT, "STATE_MSEC_AMBIGUOUS");
        int clean = constant(MEASUREMENT, "MULTIPATH_INDICATOR_NOT_DETECTED");
        List<Object> measurements = new ArrayList<>(frame.satellites().size());
        for (GnssFrame.Satellite satellite : frame.satellites()) {
            measurements.add(build(NEW_MEASUREMENT, MEASUREMENT, MEASUREMENT_FIELDS,
                    satellite.id(), GnssStatus.CONSTELLATION_GPS, 0.0, state, svTime, 50L,
                    (double) satellite.cn0(), (double) satellite.cn0() - 1.5,
                    -frame.speed(), 0.5, L1_HZ, clean, (double) satellite.cn0() - 20.0, -6.0));
        }
        return event(clock, measurements);
    }

    /**
     * 导航电文：每颗卫星一条 GPS L1 C/A 子帧，子帧号在 1..5 之间轮转。
     *
     * <p>一条接口只送一条报文，所以返回整批，由调用方逐条投递。
     */
    static List<Object> navigationMessages(GnssFrame frame) {
        int subframe = SUBFRAME.getAndUpdate(value -> value % 5 + 1) % 5 + 1;
        int type = constant(MESSAGE, "TYPE_GPS_L1CA");
        int passed = constant(MESSAGE, "STATUS_PARITY_PASSED");
        List<Object> messages = new ArrayList<>(frame.satellites().size());
        for (GnssFrame.Satellite satellite : frame.satellites()) {
            messages.add(build(NEW_MESSAGE, MESSAGE, MESSAGE_FIELDS,
                    type, satellite.id(), 0, subframe, passed, payload(satellite, subframe)));
        }
        return messages;
    }

    /** 把时钟与逐颗测量装进一个事件；Builder 是唯一能造出这个对象的入口。 */
    private static Object event(Object clock, List<Object> measurements) {
        try {
            Object builder = NEW_EVENT_BUILDER.newInstance();
            EVENT_SET_CLOCK.invoke(builder, clock);
            EVENT_SET_MEASUREMENTS.invoke(builder, measurements);
            if (EVENT_SET_FULL_TRACKING != null) EVENT_SET_FULL_TRACKING.invoke(builder, true);
            return EVENT_BUILD.invoke(builder);
        } catch (ReflectiveOperationException error) {
            throw new IllegalStateException("Cannot build GnssMeasurementsEvent", error);
        }
    }

    private static Object build(Constructor<?> constructor, Class<?> owner, Assignment[] fields, Object... values) {
        try {
            Object target = constructor.newInstance();
            for (int index = 0; index < fields.length; index++) {
                Method setter = setter(owner, fields[index].name(), fields[index].type());
                if (setter != null) setter.invoke(target, values[index]);
            }
            return target;
        } catch (ReflectiveOperationException error) {
            throw new IllegalStateException("Cannot build " + owner.getSimpleName(), error);
        }
    }

    /** 结构成立的载荷：前导码 + 卫星号 + 子帧号，其余按确定性公式铺满（不含奇偶校验）。 */
    private static byte[] payload(GnssFrame.Satellite satellite, int subframe) {
        byte[] data = new byte[SUBFRAME_BYTES];
        data[0] = (byte) 0x8B;
        data[1] = (byte) satellite.id();
        data[2] = (byte) subframe;
        for (int index = 3; index < data.length; index++) {
            data[index] = (byte) ((satellite.id() * 31 + subframe * 17 + index * 7) & 0xFF);
        }
        return data;
    }

    // ---- 反射入口 ----

    private static Class<?> load(String name) {
        try { return Class.forName(name, false, GnssRawOutput.class.getClassLoader()); }
        catch (Throwable error) { MISSING.add(name); return null; }
    }

    private static Constructor<?> constructor(Class<?> type) {
        if (type == null) return null;
        try {
            Constructor<?> constructor = type.getDeclaredConstructor();
            constructor.setAccessible(true);
            return constructor;
        } catch (Throwable error) { MISSING.add(type.getSimpleName() + "()"); return null; }
    }

    private static Method optional(Class<?> type, String name, Class<?>... parameters) {
        if (type == null) return null;
        try {
            Method method = type.getMethod(name, parameters);
            method.setAccessible(true);
            return method;
        } catch (Throwable error) { MISSING.add(type.getSimpleName() + '.' + name); return null; }
    }

    private static Method setter(Class<?> owner, String name, Class<?> type) {
        if (owner == null) return null;
        String key = name + '/' + type.getName();
        Method method = SETTERS.get(key);
        if (method != null) return method;
        try {
            method = owner.getMethod(name, type);
            method.setAccessible(true);
            SETTERS.put(key, method);
            return method;
        } catch (Throwable error) {
            String miss = owner.getSimpleName() + '.' + name;
            if (!MISSING.contains(miss)) MISSING.add(miss);
            return null;
        }
    }

    private static int constant(Class<?> owner, String name) {
        Integer value = CONSTANTS.get(name);
        if (value != null) return value;
        try {
            int resolved = owner.getField(name).getInt(null);
            CONSTANTS.put(name, resolved);
            return resolved;
        } catch (Throwable error) {
            String miss = owner.getSimpleName() + '.' + name;
            if (!MISSING.contains(miss)) MISSING.add(miss);
            return 0;
        }
    }
}
