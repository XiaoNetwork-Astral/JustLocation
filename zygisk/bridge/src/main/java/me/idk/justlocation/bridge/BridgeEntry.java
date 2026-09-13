package me.idk.justlocation.bridge;

import android.os.SystemClock;
import android.util.Log;

import java.lang.reflect.Method;

import org.json.JSONObject;

/** system_server entry point: install channels, poll state and drive periodic delivery. */
public final class BridgeEntry {
    private static final String TAG = "JustLocation";
    private static volatile LocationSnapshot fix;
    private static volatile TelephonySnapshot telephony;
    private static volatile WifiOutput wifi;
    private static final LocationHooks location =
            new LocationHooks(BridgeEntry::install, () -> fix);
    private static final GnssHooks gnss = new GnssHooks(BridgeEntry::install);
    private static TelephonyRegistryAdapter telephonyRegistry;
    private static SubscriptionRegistryHooks subscriptionRegistry;
    private static volatile WifiServiceImplHooks wifiHooks;
    private static volatile boolean cellsSynthesized;
    private static long lastDispatchError;
    private static long reportedCompanionFailures;
    private static final StepChannel steps = new StepChannel();

    private BridgeEntry() {}

    private static native String readState(int installed, int wifiCalls, String gnssRawDetail);
    private static native Method hook(Method target, Object callback, Method method);
    private static native String companionDiagnostics();
    static native boolean installSteps();
    static native void updateSteps(boolean active, boolean all, String[] packages, long total,
            long epoch, int[] handles, int[] types);

    public static void start(ClassLoader systemServerLoader) throws Exception {
        location.install(systemServerLoader);
        gnss.install(systemServerLoader);
        telephonyRegistry = TelephonyRegistryHooks.install(
                systemServerLoader, BridgeEntry::install, () -> telephony);
        subscriptionRegistry =
                SubscriptionRegistryHooks.install(systemServerLoader, () -> telephony);
        installWifiService(systemServerLoader);
        Thread thread = new Thread(BridgeEntry::run, "JustLocation-state");
        thread.setDaemon(true);
        thread.start();
    }

    private static void run() {
        while (!Thread.currentThread().isInterrupted()) {
            try {
                String response = readState((location.ready ? 1 : 0) | gnss.flags
                                | (telephonyRegistry != null ? 8 : 0) | (wifiScanReady() ? 16 : 0)
                                | (wifiConnectionReady() ? 32 : 0)
                                | (subscriptionRegistry != null && subscriptionRegistry.ready()
                                                ? 128
                                                : 0),
                        WifiServiceImplHooks.calls(), gnss.counters());
                // Keep snapshots on a lost heartbeat; expiry and explicit stop responses restore
                // real output.
                if (response != null)
                    updateState(response);
            } catch (Exception error) {
                // The native exchange already records its own failures; this path covers a
                // malformed reply and must not stay silent either.
                Log.w(TAG,
                        "Cannot apply backend state; dropping snapshots until the next valid"
                                + " heartbeat",
                        error);
                fix = null;
                telephony = null;
                wifi = null;
                steps.stop();
            }
            reportCompanionFailures();
            dispatch();
            try {
                Thread.sleep(1000);
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
            }
        }
    }

    /**
     * A lost companion descriptor cannot be rebuilt inside this process: the handshake happens
     * once per injection. Log the first occurrence with its reason and time so the silent switch
     * back to real output is traceable instead of guessed.
     */
    private static void reportCompanionFailures() {
        try {
            JSONObject report = new JSONObject(companionDiagnostics());
            long failures = report.optLong("failures");
            if (failures <= reportedCompanionFailures)
                return;
            reportedCompanionFailures = failures;
            Log.e(TAG,
                    "Companion connection lost: reason=" + report.optString("reason")
                            + " ageMs=" + report.optLong("age_ms") + " failures=" + failures
                            + "; real location output returns when the 20-second snapshot expires and"
                            + " requires a restart to recover");
        } catch (Exception error) {
            Log.w(TAG, "Cannot read companion diagnostics", error);
        }
    }

    private static void updateState(String response) throws Exception {
        if (subscriptionRegistry != null)
            subscriptionRegistry.observe(
                    new JSONObject(response).getJSONObject("state"), SystemClock.elapsedRealtime());
        steps.update(response);
        fix = !location.ready ? null : LocationSnapshot.parse(response);
        try {
            telephony = TelephonySnapshot.parse(response, SystemClock.elapsedRealtime());
        } catch (Exception error) {
            telephony = null;
        }
        try {
            wifi = WifiOutput.parse(response, SystemClock.elapsedRealtime());
        } catch (Exception error) {
            wifi = null;
        }
        // Log only changes in the cell data source.
        boolean manufactured = response.contains("\"cells_synthesized\":true");
        if (manufactured != cellsSynthesized) {
            cellsSynthesized = manufactured;
            if (manufactured) {
                Log.w(TAG,
                        "Cell output is synthesized: no real cell data covers this position;"
                                + " the reported cells exist only on this device");
            } else {
                Log.i(TAG, "Cell output is backed by real cell data again");
            }
        }
    }

    private static void dispatch() {
        try {
            if (subscriptionRegistry != null)
                subscriptionRegistry.update();
        } catch (Exception error) {
            Log.w(TAG, "Subscription change delivery failed", error);
        }
        steps.tick();
        LocationSnapshot current = fix;
        gnss.update(current);
        if (current != null) {
            try {
                location.dispatch(current);
            } catch (Exception error) {
                warnDispatch("Cannot dispatch periodic location", error);
            }
        }
        for (GnssDispatcher channel : gnss.dispatchers) {
            try {
                channel.dispatch();
            } catch (Exception error) {
                warnDispatch("Cannot dispatch GNSS channel", error);
            }
        }
        if (telephonyRegistry != null) {
            try {
                telephonyRegistry.dispatch();
            } catch (Exception error) {
                warnDispatch("Cannot dispatch cell listeners", error);
            }
        }
    }

    private static void warnDispatch(String message, Exception error) {
        long now = SystemClock.elapsedRealtime();
        if (now - lastDispatchError >= 30000) {
            lastDispatchError = now;
            Log.w(TAG, message, error);
        }
    }

    private static void installWifiService(ClassLoader systemServerLoader) {
        // Missing object APIs leave both Wi-Fi readiness flags unset.
        if (!WifiOutput.usable()) {
            Log.w(TAG, "Wi-Fi object APIs unavailable; Wi-Fi channel remains system output");
            return;
        }
        try {
            WifiServiceImplHooks hooks =
                    new WifiServiceImplHooks(systemServerLoader, packageName -> {
                        WifiOutput current = wifi;
                        if (current == null
                                || !current.appliesTo(packageName, SystemClock.elapsedRealtime()))
                            return null;
                        return current;
                    });
            if (!hooks.usable())
                throw new IllegalStateException("Wi-Fi service hooks unusable");
            hooks.install(BridgeEntry::install);
            wifiHooks = hooks;
            Log.i(TAG, "Wi-Fi service hooks installed");
        } catch (Throwable error) {
            Log.w(TAG, "Cannot install Wi-Fi service hooks; Wi-Fi channel remains system output",
                    error);
        }
    }

    private static boolean wifiScanReady() {
        WifiServiceImplHooks hooks = wifiHooks;
        return hooks != null && hooks.scanReady();
    }

    private static boolean wifiConnectionReady() {
        WifiServiceImplHooks hooks = wifiHooks;
        return hooks != null && hooks.connectionReady();
    }

    private static void install(Method target, MethodHook.Around around) throws Exception {
        MethodHook.install(target, around, BridgeEntry::hook);
    }

    public static String getBuildVersion() {
        return BuildConfig.MODULE_VERSION;
    }
}
