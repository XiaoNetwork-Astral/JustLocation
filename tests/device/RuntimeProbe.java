package me.idk.justlocation.bridge;

import dalvik.system.PathClassLoader;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;

/** Runs in a disposable adb-shell app_process, never in zygote or system_server. */
public final class RuntimeProbe {
    private static native boolean initialize();
    private static native Method hook(Method target, Object receiver, Method callback);
    private static native boolean unhook(Method target);
    private static native boolean bootstrap(String directory, byte[] dex);
    private static native boolean ready();

    public static String sample(String input) { return "original:" + input; }

    public static final class SampleHook {
        Method backup;
        public Object callback(Object[] arguments) throws Exception {
            return "hooked:" + backup.invoke(null, arguments);
        }
    }

    public static void main(String[] args) throws Throwable {
        // The native dependency is loaded by the linker, as in the Zygisk runtime.
        // System.load(shadowhook) would invoke its unrelated Android SDK JNI_OnLoad.
        if (args.length > 1 && args[1].equals("bridge")) {
            System.load(args[0] + "/libjustlocation_bootstrap_probe.so");
            PathClassLoader loader = new PathClassLoader(System.getenv("SYSTEMSERVERCLASSPATH"), ClassLoader.getSystemClassLoader());
            var cached = Class.forName("com.android.internal.os.ZygoteInit").getDeclaredField("sCachedSystemServerClassLoader");
            cached.setAccessible(true);
            cached.set(null, loader);
            require(bootstrap(args[0], Files.readAllBytes(Path.of(args[0], "classes.dex"))), "production DEX bootstrap");
            require(ready(), "provider hooks ready without intercepting subsequent class loads");
            System.out.println("JUSTLOCATION_BRIDGE_PROBE_OK");
            // Do not destroy a VM with the production bridge's daemon thread still
            // polling: normal Android processes use System.exit rather than JNI destruction.
            System.exit(0);
            return;
        }
        System.load(args[0] + "/libjustlocation_probe.so");
        require(initialize(), "LSPlant/ShadowHook initialization");
        Method sample = RuntimeProbe.class.getDeclaredMethod("sample", String.class);
        SampleHook callback = new SampleHook();
        callback.backup = hook(sample, callback, SampleHook.class.getMethod("callback", Object[].class));
        require(callback.backup != null, "install sample hook");
        require("hooked:original:probe".equals(sample.invoke(null, "probe")), "callback and original method");
        require(unhook(sample), "remove sample hook");
        require("original:probe".equals(sample.invoke(null, "probe")), "original method restored");

        PathClassLoader loader = new PathClassLoader("/system/framework/services.jar", ClassLoader.getSystemClassLoader());
        Class<?> provider = Class.forName("com.android.server.location.provider.LocationProviderManager", false, loader);
        Class<?> identity = Class.forName("android.location.util.identity.CallerIdentity", false, loader);
        identity.getMethod("getPackageName");
        provider.getMethod("getName");
        provider.getDeclaredMethod("onRegister");
        Class<?> multiplexer = Class.forName("com.android.server.location.listeners.ListenerMultiplexer", false, loader);
        multiplexer.getDeclaredMethod("deliverToListeners", java.util.function.Function.class);
        provider.getDeclaredMethod("getLastLocationUnsafe", int.class, int.class, boolean.class, long.class);
        provider.getDeclaredMethod("getLastLocation", Class.forName("android.location.LastLocationRequest", false, loader), identity, int.class);
        provider.getDeclaredMethod("getCurrentLocation", Class.forName("android.location.LocationRequest", false, loader), identity, int.class,
                Class.forName("android.location.ILocationCallback", false, loader));
        Class<?> result = Class.forName("android.location.LocationResult", false, loader);
        Class<?> registrationBase = Class.forName(provider.getName() + "$Registration", false, loader);
        registrationBase.getMethod("getIdentity");
        registrationBase.getDeclaredMethod("acceptLocationChange", result);
        result.getMethod("getLastLocation");
        result.getMethod("wrap", android.location.Location[].class);
        for (String nested : new String[]{"LocationRegistration", "GetCurrentLocationListenerRegistration"}) {
            Class<?> registration = Class.forName(provider.getName() + "$" + nested, false, loader);
            registration.getDeclaredMethod("acceptLocationChange", result);
        }
        System.out.println("PASS: Android 15 provider method signatures");
        System.out.println("JUSTLOCATION_PROBE_OK");
    }

    private static void require(boolean condition, String name) {
        if (!condition) throw new AssertionError(name);
        System.out.println("PASS: " + name);
    }
}
