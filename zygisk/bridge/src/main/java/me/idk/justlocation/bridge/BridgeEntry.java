package me.idk.justlocation.bridge;

import android.location.Location;
import android.location.GnssStatus;
import android.net.wifi.WifiInfo;
import android.os.SystemClock;
import android.util.Log;
import org.json.JSONObject;
import java.lang.reflect.Method;
import java.util.HashSet;
import java.util.Set;
import java.util.ArrayList;
import java.util.List;

/** Entry point packaged into the module; independent of the companion APK. */
public final class BridgeEntry {
    private static final String TAG = "JustLocation";
    private static final String PROVIDER = "com.android.server.location.provider.LocationProviderManager";
    private static final Set<Class<?>> installed = new HashSet<>();
    private static final ThreadLocal<String> callerPackage = new ThreadLocal<>();
    private static volatile Fix fix;
    private static volatile boolean hooksInstalled;
    private static ProviderDispatcher dispatcher;
    private static Method wrapResult;
    private static long lastDispatchError;
    /** 当前输出的基站是不是伪造的兜底数据；只用来"变化时记一条日志"。 */
    private static volatile boolean cellsSynthesized;
    private static volatile GnssListener.Output gnssOutput;
    /** 原始测量与导航电文的输出快照；与卫星状态同源（同一片天空）。 */
    private static volatile GnssRawListener.Output gnssRawOutput;
    private static final List<GnssDispatcher> gnssDispatchers = new ArrayList<>();
    private static int gnssFlags;
    private static volatile TelephonySnapshot telephony;
    private static TelephonyRegistryAdapter telephonyRegistry;
    /** Wi-Fi 服务端挂钩；装载失败时为 null，那条通道保持系统原值。 */
    private static volatile WifiServiceImplHooks wifiHooks;
    /** Wi-Fi 合成读数：与定位/卫星同一套作用范围与新鲜度规则。 */
    private static volatile WifiOutput wifi;
    private BridgeEntry() {}

    private static native String readState(int installed, int wifiCalls, String gnssRawDetail);
    private static native Method hook(Method target, Object callback, Method method);


    public static void start(ClassLoader systemServerLoader) throws Exception {
        installProvider(Class.forName(PROVIDER, false, systemServerLoader));
        installGnss(systemServerLoader, "GnssStatusProvider", "IGnssStatusListener", 2);
        installGnss(systemServerLoader, "GnssNmeaProvider", "IGnssNmeaListener", 4);
        installGnssRaw(systemServerLoader);
        installTelephonyRegistry(systemServerLoader);
        installWifiService(systemServerLoader);
        Thread thread = new Thread(() -> {
            while (!Thread.currentThread().isInterrupted()) {
                try {
                    String response = readState((hooksInstalled ? 1 : 0) | gnssFlags | (telephonyRegistry != null ? 8 : 0)
                            | (wifiScanReady() ? 16 : 0) | (wifiConnectionReady() ? 32 : 0),
                            WifiServiceImplHooks.calls(), gnssCounters());
                    // **拿不到回应时不要立刻清空快照**（2026-09-12 实测纠正）：`readState` 超时
                    // 或断线都返回 null，而那条路以前走的是"fix = null"，于是守护进程只是卡了一两秒，
                    // 应用就会读到真实坐标——3 秒窗口与 20 秒窗口都拦不住它，因为清空发生在窗口之前。
                    // 现在只由看门狗（快照过期）与明确的停止（回包里 requested_active=false）决定何时恢复真值。
                    if (response != null) {
                        fix = !hooksInstalled ? null : Fix.parse(response);
                        try { telephony = TelephonySnapshot.parse(response, SystemClock.elapsedRealtime()); }
                        catch (Exception error) { telephony = null; }
                        try { wifi = WifiOutput.parse(response, SystemClock.elapsedRealtime()); }
                        catch (Exception error) { wifi = null; }
                        // **只在状态翻转时记一条**：这一位表示"现在输出的基站是伪造的"，
                        // 每秒重复刷同一句既没信息量、又会把环形缓冲冲掉。
                        // `system_server` 写不进 /data/adb/justlocation，日志是唯一出口。
                        boolean manufactured = response.contains("\"cells_synthesized\":true");
                        if (manufactured != cellsSynthesized) {
                            cellsSynthesized = manufactured;
                            if (manufactured) {
                                Log.w(TAG, "Cell output is synthesized: no real cell data covers this position;"
                                        + " the reported cells exist only on this device");
                            } else {
                                Log.i(TAG, "Cell output is backed by real cell data again");
                            }
                        }
                    }
                } catch (Exception error) { fix = null; telephony = null; wifi = null; }
                Fix current = fix;
                GnssFrame frame = current == null ? null
                        : new GnssFrame(current.latitude, current.longitude, current.altitude,
                                current.speed, current.bearing, System.currentTimeMillis());
                gnssOutput = frame == null ? null
                        : new GnssListener.Output(current.scope, frame, current.gnssEnabled, current.nmeaEnabled);
                // 三条 GNSS 出口共用同一帧，所以卫星状态、原始测量与导航电文看到的是同一片天空。
                gnssRawOutput = frame == null ? null
                        : new GnssRawListener.Output(current.scope, frame, current.gnssEnabled);
                if (current != null) {
                    try {
                        dispatcher.dispatch(current.scope, SystemClock::elapsedRealtime, name -> {
                            try { return wrapResult.invoke(null, (Object) new Location[]{current.location(name)}); }
                            catch (Exception error) { throw new IllegalStateException(error); }
                        });
                    } catch (Exception error) {
                        long now = SystemClock.elapsedRealtime();
                        if (now - lastDispatchError >= 30000) {
                            lastDispatchError = now;
                            Log.w(TAG, "Cannot dispatch periodic location", error);
                        }
                    }
                }
                for (GnssDispatcher channel : gnssDispatchers) {
                    try { channel.dispatch(); }
                    catch (Exception error) {
                        long now = SystemClock.elapsedRealtime();
                        if (now - lastDispatchError >= 30000) {
                            lastDispatchError = now;
                            Log.w(TAG, "Cannot dispatch GNSS channel", error);
                        }
                    }
                }
                if (telephonyRegistry != null) {
                    try { telephonyRegistry.dispatch(); }
                    catch (Exception error) {
                        long now = SystemClock.elapsedRealtime();
                        if (now - lastDispatchError >= 30000) {
                            lastDispatchError = now;
                            Log.w(TAG, "Cannot dispatch cell listeners", error);
                        }
                    }
                }
                try { Thread.sleep(1000); }
                catch (InterruptedException error) { Thread.currentThread().interrupt(); }
            }
        }, "JustLocation-state");
        thread.setDaemon(true);
        thread.start();

    }



    /**
     * Wi-Fi 服务端替换：装在 `WifiServiceImpl` 自己的 classloader 上。
     *
     * <p>classloader 从 `SystemServerClassLoaderFactory.sLoadedPaths` 按 jar 路径取（见
     * {@link WifiServiceImplHooks} 的说明）。取不到就整条通道不接管——绝不用别的 classloader
     * 硬凑，那样只会得到一个看起来成功、实际挂在别的类上的假就绪。
     *
     * <p>作用范围与新鲜度沿用定位那条通道：`WifiOutput` 只负责合成对象，
     * 配置解析与门控由 `WifiSettings` / `WifiOutput` 自己决定（两者都有单测覆盖）。
     */
    private static void installWifiService(ClassLoader systemServerLoader) {
        // 合成对象需要的那几个隐藏入口（`WifiInfo`/`ScanResult`/`WifiSsid` 的构造与赋值）缺任何一个，
        // 这条通道就只会永远走"放行"分支。那种情况下**不挂钩**：两项就绪位保持 false，
        // 面板与验收能如实看到"这条通道没接管"，而不是报着就绪却一直读到系统原值。
        if (!WifiOutput.usable()) {
            Log.w(TAG, "Wi-Fi object APIs unavailable; Wi-Fi channel remains system output");
            return;
        }
        try {
            WifiServiceImplHooks hooks = new WifiServiceImplHooks(systemServerLoader, packageName -> {
                WifiOutput current = wifi;
                if (current == null || !current.appliesTo(packageName, SystemClock.elapsedRealtime())) return null;
                return current;
            });
            if (!hooks.usable()) throw new IllegalStateException("Wi-Fi service hooks unusable");
            hooks.install(BridgeEntry::install);
            wifiHooks = hooks;
            Log.i(TAG, "Wi-Fi service hooks installed");
        } catch (Throwable error) {
            Log.w(TAG, "Cannot install Wi-Fi service hooks; Wi-Fi channel remains system output", error);
        }
    }

    /** Wi-Fi 两项挂钩是否各自装上了；两项分开报告，与原版"扫描与连接信息不是同一项适配"一致。 */
    private static boolean wifiScanReady() {
        WifiServiceImplHooks hooks = wifiHooks;
        return hooks != null && hooks.scanReady();
    }

    private static boolean wifiConnectionReady() {
        WifiServiceImplHooks hooks = wifiHooks;
        return hooks != null && hooks.connectionReady();
    }

    private static void installTelephonyRegistry(ClassLoader loader) {
        try {
            Class<?> registry = Class.forName("com.android.server.TelephonyRegistry", false, loader);
            Class<?> listener = Class.forName("com.android.internal.telephony.IPhoneStateListener", false, loader);
            TelephonyRegistryAdapter adapter = new TelephonyRegistryAdapter(registry,
                    Class.forName("com.android.server.TelephonyRegistry$Record", false, loader), listener,
                    Class.forName("android.telephony.TelephonyCallback", false, loader), (pkg, sub, slot, callback, authorized) -> {
                        TelephonySnapshot current = telephony;
                        if (current == null || !current.cellsEnabled
                                || !current.appliesTo(pkg, SystemClock.elapsedRealtime())) return null;
                        int selected = current.resolveSubscription(sub, slot);
                        long timestamp = SystemClock.elapsedRealtimeNanos();
                        return switch (callback) {
                            case "onCellInfoChanged" -> current.cells(selected, timestamp);
                            case "onCellLocationChanged" -> current.identity(selected, timestamp);
                            case "onServiceStateChanged" -> authorized == null ? null
                                    : current.serviceState(selected, (android.telephony.ServiceState) authorized);
                            case "onSignalStrengthsChanged" -> current.signal(selected, timestamp);
                            case "onSignalStrengthChanged" -> {
                                int strength = (int) android.telephony.SignalStrength.class.getMethod("getGsmSignalStrength")
                                        .invoke(current.signal(selected, timestamp));
                                yield strength == 99 ? -1 : strength;
                            }
                            default -> null;
                        };
                    });
            install(registry.getDeclaredMethod("listenWithEventList", boolean.class, boolean.class, int.class,
                    String.class, String.class, listener, int[].class, boolean.class), call -> {
                call.arguments[6] = adapter.wrap(call.arguments[0], (String) call.arguments[4],
                        (int) call.arguments[3], call.arguments[6]);
                Object result = call.original();
                adapter.track(call.arguments[0]);
                return result;
            });
            telephonyRegistry = adapter;
            Log.i(TAG, "Cell listener hooks installed");
        } catch (Exception error) { Log.w(TAG, "Cell listeners unavailable", error); }
    }

    private static void installGnss(ClassLoader loader, String providerName, String listenerName, int flag) {
        try {
            Class<?> provider = Class.forName("com.android.server.location.gnss." + providerName, false, loader);
            Class<?> listener = Class.forName("android.location." + listenerName, false, loader);
            Class<?> identity = Class.forName("android.location.util.identity.CallerIdentity", false, loader);
            Class<?> registration = Class.forName("com.android.server.location.gnss.GnssListenerMultiplexer$GnssListenerRegistration", false, loader);
            Class<?> operation = Class.forName("com.android.internal.listeners.ListenerExecutor$ListenerOperation", false, loader);
            GnssDispatcher channel = new GnssDispatcher(provider, listener, registration, identity, operation,
                    (delegate, packageName) ->
                            new GnssListener(listener, delegate, packageName, () -> gnssOutput, SystemClock::elapsedRealtime,
                                    BridgeEntry::gnssStatus).proxy());
            install(provider.getDeclaredMethod("addListener", identity, listener), call -> {
                call.arguments[2] = channel.wrap(call.arguments[1], call.arguments[2]);
                Object result = call.original();
                channel.track(call.arguments[0]);
                return result;
            });
            gnssDispatchers.add(channel);
            gnssFlags |= flag;
            Log.i(TAG, providerName + " hooks installed");
        } catch (Exception error) {
            Log.w(TAG, "Cannot install " + providerName + "; location channel remains available", error);
        }
    }

    /**
     * 原始 GNSS 数据：原始测量与导航电文。
     *
     * <p>两条接口各只有一个数据回调，形状和卫星状态不同，所以走 {@link GnssRawListener}；
     * 开关沿用后台的 `gnss_enabled`——开了 GNSS 模拟就三条出口一起接管（见该类的说明）。
     *
     * <p>方法签名是**在真机上核过的**（`build/location-investigation/GnssApiProbe.java` 与对
     * `services.jar` 的 dexdump），不是照 AOSP 印象写的：这台 ROM 上
     * `GnssMeasurementsProvider.addListener` 比常见的多一个 `GnssMeasurementRequest` 参数。
     * 签名写错的话这里会抛，通道报不就绪，不会静默挂到别的东西上。
     */
    private static void installGnssRaw(ClassLoader loader) {
        // 合成对象的入口缺任何一个就整体不装：装了也只能产出半成品，还会让"就绪"变成假话。
        if (!GnssRawOutput.usable()) {
            Log.w(TAG, "Raw GNSS object APIs missing (" + GnssRawOutput.missing() + "); raw channels stay system output");
            return;
        }
        Class<?> listener, registration, identity, operation, request;
        try {
            identity = Class.forName("android.location.util.identity.CallerIdentity", false, loader);
            registration = Class.forName("com.android.server.location.gnss.GnssListenerMultiplexer$GnssListenerRegistration", false, loader);
            operation = Class.forName("com.android.internal.listeners.ListenerExecutor$ListenerOperation", false, loader);
            request = Class.forName("android.location.GnssMeasurementRequest", false, loader);
        } catch (Exception error) {
            Log.w(TAG, "Raw GNSS support classes unavailable", error);
            return;
        }
        boolean measurements = installGnssMeasurement(loader, identity, registration, operation, request);
        boolean messages = installGnssMessage(loader, identity, registration, operation);
        if (measurements && messages) gnssFlags |= 64;
        Log.i(TAG, "Raw GNSS hooks: measurements=" + measurements + " navigationMessages=" + messages);
    }

    /** 计数串每次心跳重算：它只在诊断时看，代价是每秒钟几个小字符串。
     *
     * <p>为什么 GNSS 要有这个：这轮"导航电文送不到"卡住的原因，正是"钩子被走到"与
     * "应用真的收到"之间隔着好几步，而 `system_server` 既写不进 `/data/adb/justlocation`、
     * 日志又会被环形缓冲冲掉（备忘录的通用经验）。计数走状态回包本身，没有写入失败这回事，
     * 也不用翻日志：**注册数 > 0 而送达数 = 0，就说明断在投递，而不是没挂上钩子**。
     */
    private static String gnssCounters() {
        StringBuilder counters = new StringBuilder();
        for (int index = 0; index < gnssDispatchers.size(); index++) {
            if (counters.length() > 0) counters.append(';');
            counters.append(index).append(':').append(String.join(",", gnssDispatchers.get(index).counters()));
        }
        return counters.toString();
    }

    private static boolean installGnssMeasurement(ClassLoader loader, Class<?> identity, Class<?> registration,
                                                  Class<?> operation, Class<?> request) {
        try {
            Class<?> provider = Class.forName("com.android.server.location.gnss.GnssMeasurementsProvider", false, loader);
            Class<?> listener = Class.forName("android.location.IGnssMeasurementsListener", false, loader);
            GnssDispatcher channel = new GnssDispatcher(provider, listener, registration, identity, operation,
                    (delegate, packageName) ->
                            new GnssRawListener(listener, delegate, packageName, () -> gnssRawOutput, SystemClock::elapsedRealtime,
                                    "onGnssMeasurementsReceived",
                                    frame -> java.util.List.of(GnssRawOutput.measurements(frame))).proxy(),
                    // 自己驱动投递：这两条出口的数据全部由我们合成，不该等 HAL 来叫（见 GnssDispatcher 的说明）。
                    true);
            install(provider.getDeclaredMethod("addListener", request, identity, listener), call -> {
                call.arguments[3] = channel.wrap(call.arguments[2], call.arguments[3]);
                Object result = call.original();
                channel.track(call.arguments[0]);
                return result;
            });
            gnssDispatchers.add(channel);
            return true;
        } catch (Throwable error) {
            Log.w(TAG, "Cannot install raw GNSS measurements; that channel stays system output", error);
            return false;
        }
    }

    private static boolean installGnssMessage(ClassLoader loader, Class<?> identity, Class<?> registration, Class<?> operation) {
        try {
            Class<?> provider = Class.forName("com.android.server.location.gnss.GnssNavigationMessageProvider", false, loader);
            Class<?> listener = Class.forName("android.location.IGnssNavigationMessageListener", false, loader);
            GnssDispatcher channel = new GnssDispatcher(provider, listener, registration, identity, operation,
                    (delegate, packageName) ->
                            new GnssRawListener(listener, delegate, packageName, () -> gnssRawOutput, SystemClock::elapsedRealtime,
                                    "onGnssNavigationMessageReceived",
                                    frame -> new ArrayList<Object>(GnssRawOutput.navigationMessages(frame))).proxy(),
                    // 同上：导航电文是这轮唯一"钩子挂上了却送不到"的出口，不能依赖 HAL 触发。
                    true);
            install(provider.getDeclaredMethod("addListener", identity, listener), call -> {
                call.arguments[2] = channel.wrap(call.arguments[1], call.arguments[2]);
                Object result = call.original();
                channel.track(call.arguments[0]);
                return result;
            });
            gnssDispatchers.add(channel);
            return true;
        } catch (Throwable error) {
            Log.w(TAG, "Cannot install GNSS navigation messages; that channel stays system output", error);
            return false;
        }
    }

    private static Object gnssStatus(GnssFrame frame) {
        GnssStatus.Builder builder = new GnssStatus.Builder();
        for (GnssFrame.Satellite satellite : frame.satellites()) {
            builder.addSatellite(GnssStatus.CONSTELLATION_GPS, satellite.id(), satellite.cn0(),
                    satellite.elevation(), satellite.azimuth(), true, true, true, true, 1575420000f, false, 0);
        }
        return builder.build();
    }

    private static void install(Method target, MethodHook.Around around) throws Exception {
        target.setAccessible(true);
        MethodHook callback = new MethodHook(target, around);
        // A concurrently entering callback waits until its backup is published.
        synchronized (callback) {
            Method backup = hook(target, callback, MethodHook.class.getMethod("callback", Object[].class));
            if (backup == null) throw new IllegalStateException("Cannot hook " + target);
            callback.setBackup(backup);
        }
    }

    private static synchronized void installProvider(Class<?> type) throws Exception {
        if (!installed.add(type)) return;
        ClassLoader loader = type.getClassLoader();
        Class<?> request = Class.forName("android.location.LastLocationRequest", false, loader);
        Class<?> identity = Class.forName("android.location.util.identity.CallerIdentity", false, loader);
        Method packageName = identity.getMethod("getPackageName");
        Method getName = type.getMethod("getName");
        Class<?> resultType = Class.forName("android.location.LocationResult", false, loader);
        wrapResult = resultType.getMethod("wrap", Location[].class);
        dispatcher = new ProviderDispatcher(type, Class.forName(PROVIDER + "$Registration", false, loader), resultType, identity);
        install(type.getDeclaredMethod("onRegister"), call -> {
            Object result = call.original();
            dispatcher.track(call.arguments[0]);
            return result;
        });
        Method getLast = type.getDeclaredMethod("getLastLocation", request, identity, int.class);
        Method unsafe = type.getDeclaredMethod("getLastLocationUnsafe", int.class, int.class, boolean.class, long.class);
        install(unsafe, call -> {
            Object original = call.original();
            Fix current = fix;
            if (current == null || !current.scope.appliesTo(callerPackage.get(), SystemClock.elapsedRealtime())) return original;
            try { return current.location((String) getName.invoke(call.arguments[0])); }
            catch (Exception error) { Log.w(TAG, "Cannot construct location", error); return original; }
        });
        install(getLast, call -> {
            String previous = callerPackage.get();
            try {
                callerPackage.set((String) packageName.invoke(call.arguments[2]));
                return call.original();
            } finally {
                if (previous == null) callerPackage.remove(); else callerPackage.set(previous);
            }
        });
        Method current = type.getDeclaredMethod("getCurrentLocation",
                Class.forName("android.location.LocationRequest", false, loader), identity, int.class,
                Class.forName("android.location.ILocationCallback", false, loader));
        install(current, call -> {
            String previous = callerPackage.get();
            try {
                callerPackage.set((String) packageName.invoke(call.arguments[2]));
                return call.original();
            } finally {
                if (previous == null) callerPackage.remove(); else callerPackage.set(previous);
            }
        });
        installDelivery(Class.forName(PROVIDER + "$LocationRegistration", false, loader), packageName, loader);
        installDelivery(Class.forName(PROVIDER + "$GetCurrentLocationListenerRegistration", false, loader), packageName, loader);
        hooksInstalled = true;
        Log.i(TAG, "Android 15 location hooks and periodic delivery installed");
    }

    private static void installDelivery(Class<?> registration, Method packageName, ClassLoader loader) throws Exception {
        Class<?> resultType = Class.forName("android.location.LocationResult", false, loader);
        Method accept = registration.getDeclaredMethod("acceptLocationChange", resultType);
        Method identity = declaredMethod(registration, "getIdentity");
        Method last = resultType.getMethod("getLastLocation");
        Method wrap = resultType.getMethod("wrap", Location[].class);
        install(accept, call -> {
            Object originalResult = call.arguments[1];
            Fix current = fix;
            if (current != null && originalResult != null) {
                try {
                    String caller = (String) packageName.invoke(identity.invoke(call.arguments[0]));
                    if (current.scope.appliesTo(caller, SystemClock.elapsedRealtime())) {
                        Location previous = (Location) last.invoke(originalResult);
                        Location replacement = current.location(previous.getProvider());
                        call.arguments[1] = wrap.invoke(null, (Object) new Location[]{replacement});
                    }
                } catch (Exception error) { Log.w(TAG, "Cannot replace callback location", error); }
            }
            // Existing permission, expiry, AppOps and coarse-location logic still runs.
            return call.original();
        });
    }

    private static Method declaredMethod(Class<?> type, String name) throws NoSuchMethodException {
        for (Class<?> current = type; current != null; current = current.getSuperclass()) {
            try {
                Method method = current.getDeclaredMethod(name);
                method.setAccessible(true);
                return method;
            } catch (NoSuchMethodException ignored) { }
        }
        throw new NoSuchMethodException(type.getName() + "." + name);
    }

    private static final class Fix {
        final SessionSnapshot scope;
        final double latitude, longitude, altitude;
        final float accuracy, speed, bearing;
        /** 卫星两个通道的开关。旧后台不返回这一段时按关闭处理，这样默认就是系统原样。 */
        final boolean gnssEnabled, nmeaEnabled;
        Fix(SessionSnapshot scope, JSONObject position) throws Exception { this(scope, position, false, false); }
        Fix(SessionSnapshot scope, JSONObject position, boolean gnssEnabled, boolean nmeaEnabled) throws Exception {
            this.scope = scope;
            this.gnssEnabled = gnssEnabled; this.nmeaEnabled = nmeaEnabled;
            latitude = position.getDouble("latitude"); longitude = position.getDouble("longitude");
            altitude = position.getDouble("altitude"); accuracy = (float) position.getDouble("accuracy");
            speed = (float) position.getDouble("speed"); bearing = (float) position.getDouble("bearing");
            if (!Double.isFinite(latitude) || Math.abs(latitude) > 90 || !Double.isFinite(longitude) || Math.abs(longitude) > 180
                    || !Double.isFinite(altitude) || !Float.isFinite(accuracy) || accuracy < 0
                    || !Float.isFinite(speed) || speed < 0 || !Float.isFinite(bearing) || bearing < 0 || bearing >= 360) {
                throw new IllegalArgumentException("Invalid backend position");
            }
        }
        static Fix parse(String response) throws Exception {
            JSONObject json = new JSONObject(response);
            if (json.getInt("version") != 1 || !json.getBoolean("ok")) return null;
            JSONObject state = json.getJSONObject("state");
            if (!state.getBoolean("requested_active")) return null;
            JSONObject config = state.getJSONObject("config");
            JSONObject selection = config.getJSONObject("scope");
            String mode = selection.getString("mode");
            Set<String> packages = new HashSet<>();
            if (mode.equals("apps")) {
                var list = selection.getJSONArray("packages");
                for (int i = 0; i < list.length(); i++) packages.add(list.getString(i));
            } else if (!mode.equals("all")) return null;
            // 卫星开关缺失或类型不对时按关闭处理：宁可让应用看到系统原样，
            // 也不要因为一段坏配置而持续投递合成卫星数据。
            boolean gnssEnabled = false, nmeaEnabled = false;
            JSONObject gnss = state.optJSONObject("gnss");
            if (gnss != null) {
                gnssEnabled = gnss.optBoolean("gnss_enabled", false);
                nmeaEnabled = gnss.optBoolean("nmea_enabled", false);
            }
            return new Fix(new SessionSnapshot(true, mode.equals("all"), packages, SystemClock.elapsedRealtime()),
                    config.getJSONObject("position"), gnssEnabled, nmeaEnabled);
        }
        Location location(String provider) {
            Location result = new Location(provider);
            result.setLatitude(latitude); result.setLongitude(longitude); result.setAltitude(altitude);
            result.setAccuracy(accuracy); result.setSpeed(speed); result.setBearing(bearing);
            result.setTime(System.currentTimeMillis()); result.setElapsedRealtimeNanos(SystemClock.elapsedRealtimeNanos());
            return result;
        }
    }

    public static String getBuildVersion() {
        return "0.1.0";
    }
}
