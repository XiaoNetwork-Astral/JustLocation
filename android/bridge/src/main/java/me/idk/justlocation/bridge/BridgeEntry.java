package me.idk.justlocation.bridge;

import android.location.Location;
import android.location.GnssStatus;
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
    private BridgeEntry() {}

    private static native String readState(int installed);
    private static native Method hook(Method target, Object callback, Method method);

    public static void start(ClassLoader systemServerLoader) throws Exception {
        installProvider(Class.forName(PROVIDER, false, systemServerLoader));
        installGnss(systemServerLoader, "GnssStatusProvider", "IGnssStatusListener", 2);
        installGnss(systemServerLoader, "GnssNmeaProvider", "IGnssNmeaListener", 4);
        installTelephonyRegistry(systemServerLoader);
        Thread thread = new Thread(() -> {
            while (!Thread.currentThread().isInterrupted()) {
                try {
                    String response = readState((hooksInstalled ? 1 : 0) | gnssFlags | (telephonyRegistry != null ? 8 : 0));
                    fix = response == null || !hooksInstalled ? null : Fix.parse(response);
                    try { telephony = TelephonySnapshot.parse(response, SystemClock.elapsedRealtime()); }
                    catch (Exception error) { telephony = null; }
                } catch (Exception error) { fix = null; telephony = null; }
                Fix current = fix;
                gnssOutput = current == null ? null : new GnssListener.Output(current.scope,
                        new GnssFrame(current.latitude, current.longitude, current.altitude, current.speed, current.bearing, System.currentTimeMillis()));
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
        Fix(SessionSnapshot scope, JSONObject position) throws Exception {
            this.scope = scope;
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
            return new Fix(new SessionSnapshot(true, mode.equals("all"), packages, SystemClock.elapsedRealtime()), config.getJSONObject("position"));
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
