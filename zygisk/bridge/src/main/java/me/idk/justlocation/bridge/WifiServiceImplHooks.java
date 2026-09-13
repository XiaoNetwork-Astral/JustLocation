package me.idk.justlocation.bridge;

import android.net.wifi.ScanResult;
import android.net.wifi.WifiInfo;
import android.os.SystemClock;

import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.List;
import java.util.Map;
import java.util.function.Function;

/**
 * Install Wi-Fi hooks with separate readiness for connection and scan output. WifiServiceImpl uses
 * an APEX classloader retrieved by jar path from SystemServerClassLoaderFactory.sLoadedPaths.
 * System UIDs pass through; application calls use the configured scope and snapshot age.
 */
final class WifiServiceImplHooks {
    /** Wi-Fi service jar with its own classloader. */
    private static final String SERVICE_JAR = "/apex/com.android.wifi/javalib/service-wifi.jar";
    private static final String FACTORY = "com.android.internal.os.SystemServerClassLoaderFactory";
    private static final String SERVICE = "com.android.server.wifi.WifiServiceImpl";
    private static final String APEX_SLICE =
            "com.android.wifi.x.com.android.modules.utils.ParceledListSlice";

    private final Class<?> service;
    private final Method connectionInfo;
    private final Method scanResults;
    private final Constructor<?> slice;
    private final WifiPermissions permissions;
    private final Function<String, WifiOutput> source;
    private volatile boolean scanReady, connectionReady;

    /**
     * Count hook calls in heartbeat state so diagnostics do not depend on system_server file
     * access or retained logcat output.
     */
    private static final java.util.concurrent.atomic.AtomicInteger CALLS =
            new java.util.concurrent.atomic.AtomicInteger();

    static int calls() {
        return CALLS.get();
    }

    /** Count both pass-through and replacement calls. */
    private static void record() {
        CALLS.incrementAndGet();
    }

    WifiServiceImplHooks(ClassLoader systemServerLoader, Function<String, WifiOutput> source)
            throws Exception {
        this.source = source;
        ClassLoader serviceLoader = serviceClassLoader(systemServerLoader);
        if (serviceLoader == null)
            throw new ClassNotFoundException(
                    "Wi-Fi service classloader is not registered in SystemServerClassLoaderFactory");
        service = Class.forName(SERVICE, false, serviceLoader);
        connectionInfo = service.getDeclaredMethod("getConnectionInfo", String.class, String.class);
        scanResults = service.getDeclaredMethod("getScanResults", String.class, String.class);
        // Constructor lookup throws when unavailable.
        slice = Class.forName(APEX_SLICE, false, serviceLoader).getConstructor(List.class);
        permissions = new WifiPermissions(service);
    }

    /**
     * Read the registered classloader without creating a replacement. getOrCreateClassLoader
     * restricts runtime creation for APEX jars; a missing table entry returns null.
     */
    private static ClassLoader serviceClassLoader(ClassLoader systemServerLoader) {
        try {
            Class<?> factory = Class.forName(FACTORY, false, systemServerLoader);
            Field loaded = factory.getDeclaredField("sLoadedPaths");
            loaded.setAccessible(true);
            Object value = loaded.get(null);
            if (!(value instanceof Map<?, ?> map))
                return null;
            // Some ROMs append an ABI suffix to the registered jar path.
            Object exact = map.get(SERVICE_JAR);
            if (exact instanceof ClassLoader found)
                return found;
            for (Map.Entry<?, ?> entry : map.entrySet()) {
                if (entry.getKey() instanceof String key && key.startsWith(SERVICE_JAR)
                        && entry.getValue() instanceof ClassLoader found) {
                    return found;
                }
            }
            return null;
        } catch (Throwable error) {
            android.util.Log.w("JustLocation", "Cannot read SystemServerClassLoaderFactory", error);
            return null;
        }
    }

    boolean scanReady() {
        return scanReady;
    }
    boolean connectionReady() {
        return connectionReady;
    }

    /** A missing service class leaves the channel disabled. */
    boolean usable() {
        return service != null && connectionInfo != null && scanResults != null && slice != null;
    }

    /**
     * System UIDs below 10000 pass through. SystemUI and Settings may use application UIDs and
     * therefore still depend on scope selection. Read the calling UID only on the hook's Binder
     * thread; direct local calls use the system_server UID.
     */
    private static boolean systemCaller() {
        return android.os.Binder.getCallingUid() < android.os.Process.FIRST_APPLICATION_UID;
    }

    void install(MethodHook.Installer installer) throws Exception {
        installer.install(connectionInfo, call -> {
            record();
            String packageName = call.arguments[1] instanceof String name ? name : null;
            WifiOutput output = systemCaller() ? null : current(packageName);
            if (output == null)
                return call.original();
            // Run the system implementation first to retain its permission and state checks.
            Object original = call.original();
            int uid = android.os.Binder.getCallingUid();
            long token = android.os.Binder.clearCallingIdentity();
            try {
                if (original == null
                        || !permissions.scanAllowed(
                                call.arguments[0], packageName, (String) call.arguments[2], uid))
                    return original;
                output = current(packageName);
                if (output == null)
                    return original;
                WifiInfo replacement = output.connectionInfo();
                return replacement == null
                        ? original
                        : replacement.makeCopy(permissions.redactions(
                                  call.arguments[0], uid, replacement.getApplicableRedactions()));
            } catch (Exception error) {
                android.util.Log.w("JustLocation", "Cannot build Wi-Fi connection info", error);
                return original;
            } finally {
                android.os.Binder.restoreCallingIdentity(token);
            }
        });
        connectionReady = true;
        installer.install(scanResults, call -> {
            record();
            String packageName = call.arguments[1] instanceof String name ? name : null;
            WifiOutput output = systemCaller() ? null : current(packageName);
            if (output == null)
                return call.original();
            Object original = call.original();
            int uid = android.os.Binder.getCallingUid();
            long token = android.os.Binder.clearCallingIdentity();
            try {
                if (original == null
                        || !permissions.scanAllowed(
                                call.arguments[0], packageName, (String) call.arguments[2], uid))
                    return original;
                output = current(packageName);
                if (output == null)
                    return original;
                // Construct the APEX-specific ParceledListSlice with the service classloader.
                return slice.newInstance(output.scanResults(SystemClock.elapsedRealtimeNanos()));
            } catch (Exception error) {
                android.util.Log.w("JustLocation", "Cannot build Wi-Fi scan results", error);
                return original;
            } finally {
                android.os.Binder.restoreCallingIdentity(token);
            }
        });
        scanReady = true;
    }

    private WifiOutput current(String packageName) {
        if (packageName == null)
            return null;
        // WifiOutput applies package scope and snapshot age checks.
        return source.apply(packageName);
    }
}
