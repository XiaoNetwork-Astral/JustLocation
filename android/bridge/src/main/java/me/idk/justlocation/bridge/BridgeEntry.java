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
    private static volatile GnssListener.Output gnssOutput;
    private static final List<GnssDispatcher> gnssDispatchers = new ArrayList<>();
    private static int gnssFlags;
    private static volatile TelephonySnapshot telephony;
    private static TelephonyRegistryAdapter telephonyRegistry;
    /** Wi-Fi 合成读数：与定位/卫星同一套作用范围与新鲜度规则。 */
    private static volatile WifiOutput wifi;
    /** 反射构造 ParceledListSlice：Wi-Fi 服务的返回类型在 framework 的混淆包里。 */
    private static java.lang.reflect.Constructor<?> listSlice;
    private BridgeEntry() {}

    private static native String readState(int installed);
    private static native Method hook(Method target, Object callback, Method method);

    /** Wi-Fi 服务实现类的全名；ROM 上加载它的 classloader 与 system_server 的那个不是同一个。 */
    private static final String WIFI_SERVICE = "com.android.server.wifi.WifiServiceImpl";
    /** 真正加载了 Wi-Fi 服务类的那个 classloader；拿到它才能装 Wi-Fi hook。 */
    private static volatile ClassLoader wifiLoader;

    public static void start(ClassLoader systemServerLoader) throws Exception {
        installProvider(Class.forName(PROVIDER, false, systemServerLoader));
        installGnss(systemServerLoader, "GnssStatusProvider", "IGnssStatusListener", 2);
        installGnss(systemServerLoader, "GnssNmeaProvider", "IGnssNmeaListener", 4);
        installTelephonyRegistry(systemServerLoader);
        // Wi-Fi 服务类由 Wi-Fi APEX 的 classloader 加载，system_server 那个 classloader 的
        // DexPathList 里没有它的 jar（本机实测），因此这里按类名加载必然失败。
        // `wifiLoader` 要等"接住 Wi-Fi 服务实例"那条路做出来才会被填上；在那之前
        // Wi-Fi 通道保持系统原值，不会输出任何合成读数。
        installWifi(wifiLoader);
        Thread thread = new Thread(() -> {
            while (!Thread.currentThread().isInterrupted()) {
                try {
                    String response = readState((hooksInstalled ? 1 : 0) | gnssFlags | (telephonyRegistry != null ? 8 : 0));
                    fix = response == null || !hooksInstalled ? null : Fix.parse(response);
                    try { telephony = TelephonySnapshot.parse(response, SystemClock.elapsedRealtime()); }
                    catch (Exception error) { telephony = null; }
                    try { wifi = WifiOutput.parse(response, SystemClock.elapsedRealtime()); }
                    catch (Exception error) { wifi = null; }
                } catch (Exception error) { fix = null; telephony = null; wifi = null; }
                Fix current = fix;
                gnssOutput = current == null ? null : new GnssListener.Output(current.scope,
                        new GnssFrame(current.latitude, current.longitude, current.altitude, current.speed, current.bearing, System.currentTimeMillis()),
                        current.gnssEnabled, current.nmeaEnabled);
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
     * 安装 Wi-Fi 的两处 Hook：当前连接信息与扫描结果。
     *
     * <p>两个目标方法自己会先做权限检查（`enforceAccessPermission`、
     * `enforceCanAccessScanResults`）再取数据，替换的是它们的**返回值**，
     * 因此调用方该有的权限与会抛出的异常都照旧。
     *
     * <p>**当前状态：调用不到。** 本机（HyperOS / Android 15 主line Wi-Fi）实测：
     * Wi-Fi 服务的实现类由 Wi-Fi APEX 的 classloader 加载，`system_server` 自身
     * classloader 的 DexPathList 里没有 `/apex/com.android.wifi/javalib/service-wifi.jar`
     * （列出的 15 个 APEX service jar 里没有它），boot classloader 也看不见，
     * 所以按类名加载只会 ClassNotFoundException。
     *
     * <p>也试过挂钩 `ClassLoader.loadClass` 等系统自己加载它的那一刻：整个开机过程该方法
     * 只被走到个位数次（`android.miui.R` 这类），一次 Wi-Fi 类都没有——ART 内部解析类
     * 基本不走这个 Java 方法，所以这条路不成立。
     *
     * <p>可行的下一步是**接住服务实例**：Wi-Fi 服务在启动期间向系统注册，实例就在注册
     * 调用的参数里，拿到实例即拿到它的 Class，再走这里现有的挂钩逻辑即可。
     */
    private static void installWifi(ClassLoader loader) {
        if (loader == null) {
            // 还没有拿到 APEX 那个 classloader；Wi-Fi 通道保持系统原值。
            return;
        }
        try {
            Class<?> service = Class.forName(WIFI_SERVICE, false, loader);
            Class<?> info = Class.forName("android.net.wifi.WifiInfo", false, loader);
            Class<?> scan = Class.forName("android.net.wifi.ScanResult", false, loader);
            Class<?> slice = Class.forName(
                    "com.android.wifi.x.com.android.modules.utils.ParceledListSlice", false, loader);
            listSlice = slice.getConstructor(List.class);
            install(service.getDeclaredMethod("getConnectionInfo", String.class, String.class), call -> {
                WifiOutput current = wifi;
                if (current != null && current.appliesTo((String) call.arguments[1], SystemClock.elapsedRealtime())) {
                    Object result = call.original();
                    // Wi-Fi 关闭时系统返回一个空对象，它的 SSID 就是占位名；
                    // 这种情况下不接管，否则关掉 Wi-Fi 反而会凭空出现一个"已连接"的网络。
                    if (result != null && !WifiSettings.isPlaceholder(((WifiInfo) result).getSSID())) {
                        call.arguments[0] = current.connectionInfo();
                        return call.original();
                    }
                    return result;
                }
                return call.original();
            });
            install(service.getDeclaredMethod("getScanResults", String.class, String.class), call -> {
                WifiOutput current = wifi;
                if (current == null || !current.appliesTo((String) call.arguments[0], SystemClock.elapsedRealtime())) return call.original();
                // 泛型在擦除后与方法的参数类型一致：传一个 ArrayList<ScanResult> 进去即可。
                return listSlice.newInstance(current.scanResults(SystemClock.elapsedRealtime()));
            });
            Log.i(TAG, "Wi-Fi service hooks installed");
        } catch (Exception error) {
            Log.w(TAG, "Cannot install Wi-Fi hooks; Wi-Fi channel remains system output", error);
        }
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
                    () -> gnssOutput, SystemClock::elapsedRealtime, BridgeEntry::gnssStatus);
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
