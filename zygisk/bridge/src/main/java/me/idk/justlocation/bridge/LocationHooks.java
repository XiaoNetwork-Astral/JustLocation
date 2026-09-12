package me.idk.justlocation.bridge;

import android.location.Location;
import android.os.SystemClock;
import android.util.Log;

import java.lang.reflect.Method;
import java.util.HashSet;
import java.util.Set;
import java.util.function.Supplier;

/** Location queries, callback replacement and periodic delivery share the caller scope. */
final class LocationHooks {
    private static final String TAG = "JustLocation";
    private static final String PROVIDER =
            "com.android.server.location.provider.LocationProviderManager";
    private final Set<Class<?>> installed = new HashSet<>();
    private final ThreadLocal<String> callerPackage = new ThreadLocal<>();
    private final MethodHook.Installer installer;
    private final Supplier<LocationSnapshot> snapshot;
    private ProviderDispatcher dispatcher;
    private Method wrapResult;
    boolean ready;

    LocationHooks(MethodHook.Installer installer, Supplier<LocationSnapshot> snapshot) {
        this.installer = installer;
        this.snapshot = snapshot;
    }

    void install(ClassLoader loader) throws Exception {
        installProvider(Class.forName(PROVIDER, false, loader));
    }

    void dispatch(LocationSnapshot current) throws Exception {
        dispatcher.dispatch(current.scope, SystemClock::elapsedRealtime, name -> {
            try {
                return wrapResult.invoke(null, (Object) new Location[] {current.location(name)});
            } catch (Exception error) {
                throw new IllegalStateException(error);
            }
        });
    }

    private synchronized void installProvider(Class<?> type) throws Exception {
        if (!installed.add(type))
            return;
        ClassLoader loader = type.getClassLoader();
        Class<?> request = Class.forName("android.location.LastLocationRequest", false, loader);
        Class<?> identity =
                Class.forName("android.location.util.identity.CallerIdentity", false, loader);
        Method packageName = identity.getMethod("getPackageName");
        Method getName = type.getMethod("getName");
        Class<?> resultType = Class.forName("android.location.LocationResult", false, loader);
        wrapResult = resultType.getMethod("wrap", Location[].class);
        dispatcher = new ProviderDispatcher(type,
                Class.forName(PROVIDER + "$Registration", false, loader), resultType, identity);
        installer.install(type.getDeclaredMethod("onRegister"), call -> {
            Object result = call.original();
            dispatcher.track(call.arguments[0]);
            return result;
        });
        Method getLast = type.getDeclaredMethod("getLastLocation", request, identity, int.class);
        Method unsafe = type.getDeclaredMethod(
                "getLastLocationUnsafe", int.class, int.class, boolean.class, long.class);
        installer.install(unsafe, call -> {
            Object original = call.original();
            LocationSnapshot current = snapshot.get();
            if (current == null
                    || !current.scope.appliesTo(callerPackage.get(), SystemClock.elapsedRealtime()))
                return original;
            try {
                return current.location((String) getName.invoke(call.arguments[0]));
            } catch (Exception error) {
                Log.w(TAG, "Cannot construct location", error);
                return original;
            }
        });
        installer.install(getLast, call -> {
            String previous = callerPackage.get();
            try {
                callerPackage.set((String) packageName.invoke(call.arguments[2]));
                return call.original();
            } finally {
                if (previous == null)
                    callerPackage.remove();
                else
                    callerPackage.set(previous);
            }
        });
        Method current = type.getDeclaredMethod("getCurrentLocation",
                Class.forName("android.location.LocationRequest", false, loader), identity,
                int.class, Class.forName("android.location.ILocationCallback", false, loader));
        installer.install(current, call -> {
            String previous = callerPackage.get();
            try {
                callerPackage.set((String) packageName.invoke(call.arguments[2]));
                return call.original();
            } finally {
                if (previous == null)
                    callerPackage.remove();
                else
                    callerPackage.set(previous);
            }
        });
        installDelivery(Class.forName(PROVIDER + "$LocationRegistration", false, loader),
                packageName, loader);
        installDelivery(
                Class.forName(PROVIDER + "$GetCurrentLocationListenerRegistration", false, loader),
                packageName, loader);
        ready = true;
        Log.i(TAG, "Android 15 location hooks and periodic delivery installed");
    }

    private void installDelivery(Class<?> registration, Method packageName, ClassLoader loader)
            throws Exception {
        Class<?> resultType = Class.forName("android.location.LocationResult", false, loader);
        Method accept = registration.getDeclaredMethod("acceptLocationChange", resultType);
        Method identity = declaredMethod(registration, "getIdentity");
        Method last = resultType.getMethod("getLastLocation");
        Method wrap = resultType.getMethod("wrap", Location[].class);
        installer.install(accept, call -> {
            Object originalResult = call.arguments[1];
            LocationSnapshot current = snapshot.get();
            if (current != null && originalResult != null) {
                try {
                    String caller = (String) packageName.invoke(identity.invoke(call.arguments[0]));
                    if (current.scope.appliesTo(caller, SystemClock.elapsedRealtime())) {
                        Location previous = (Location) last.invoke(originalResult);
                        Location replacement = current.location(previous.getProvider());
                        call.arguments[1] =
                                wrap.invoke(null, (Object) new Location[] {replacement});
                    }
                } catch (Exception error) {
                    Log.w(TAG, "Cannot replace callback location", error);
                }
            }
            // Existing permission, expiry, AppOps and coarse-location logic still runs.
            return call.original();
        });
    }

    private Method declaredMethod(Class<?> type, String name) throws NoSuchMethodException {
        for (Class<?> current = type; current != null; current = current.getSuperclass()) {
            try {
                Method method = current.getDeclaredMethod(name);
                method.setAccessible(true);
                return method;
            } catch (NoSuchMethodException ignored) {
            }
        }
        throw new NoSuchMethodException(type.getName() + "." + name);
    }
}
