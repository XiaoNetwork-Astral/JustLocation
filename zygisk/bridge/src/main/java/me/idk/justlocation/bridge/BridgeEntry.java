package me.idk.justlocation.bridge;

import android.os.SystemClock;
import android.util.Log;

import java.lang.reflect.Method;

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
    private static volatile WifiServiceImplHooks wifiHooks;
    private static volatile boolean cellsSynthesized;
    private static long lastDispatchError;
    private static final StepChannel steps = new StepChannel();

    private BridgeEntry() {}

    private static native String readState(int installed, int wifiCalls, String gnssRawDetail);
    private static native Method hook(Method target, Object callback, Method method);
    static native boolean installSteps();
    static native void updateSteps(boolean active, boolean all, String[] packages, long total,
            long epoch, int[] handles, int[] types);

    public static void start(ClassLoader systemServerLoader) throws Exception {
        location.install(systemServerLoader);
        gnss.install(systemServerLoader);
        telephonyRegistry = TelephonyRegistryHooks.install(
                systemServerLoader, BridgeEntry::install, () -> telephony);
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
                                | (wifiConnectionReady() ? 32 : 0),
                        WifiServiceImplHooks.calls(), gnss.counters());
                // Keep snapshots on a lost heartbeat; expiry and explicit stop responses restore
                // real output.
                if (response != null)
                    updateState(response);
            } catch (Exception error) {
                fix = null;
                telephony = null;
                wifi = null;
                steps.stop();
            }
            dispatch();
            try {
                Thread.sleep(1000);
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
            }
        }
    }

    private static void updateState(String response) throws Exception {
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
        return "0.1.0";
    }
}
