package me.idk.justlocation.bridge;

import android.os.SystemClock;
import android.util.Log;

import java.util.function.Supplier;

/** Install system_server cell listeners and adapt their authorized Android output. */
final class TelephonyRegistryHooks {
    private static final String TAG = "JustLocation";

    private TelephonyRegistryHooks() {}

    static TelephonyRegistryAdapter install(ClassLoader loader, MethodHook.Installer installer,
            Supplier<TelephonySnapshot> snapshot) {
        try {
            Class<?> registry =
                    Class.forName("com.android.server.TelephonyRegistry", false, loader);
            Class<?> listener = Class.forName(
                    "com.android.internal.telephony.IPhoneStateListener", false, loader);
            TelephonyRegistryAdapter adapter = new TelephonyRegistryAdapter(registry,
                    Class.forName("com.android.server.TelephonyRegistry$Record", false, loader),
                    listener, Class.forName("android.telephony.TelephonyCallback", false, loader),
                    (pkg, sub, slot, callback, authorized) -> {
                        TelephonySnapshot current = snapshot.get();
                        if (current == null || !current.cellsEnabled
                                || !current.appliesTo(pkg, SystemClock.elapsedRealtime()))
                            return null;
                        int selected = current.resolveSubscription(sub, slot);
                        long timestamp = SystemClock.elapsedRealtimeNanos();
                        return switch (callback) {
                            case "onCellInfoChanged" -> current.cells(selected, timestamp);
                            case "onCellLocationChanged" -> current.identity(selected, timestamp);
                            case "onServiceStateChanged" ->
                                authorized == null
                                        ? null
                                        : current.serviceState(selected,
                                                  (android.telephony.ServiceState) authorized);
                            case "onSignalStrengthsChanged" -> current.signal(selected, timestamp);
                            case "onSignalStrengthChanged" -> {
                                int strength = (int) android.telephony.SignalStrength
                                                       .class.getMethod("getGsmSignalStrength")
                                                       .invoke(current.signal(selected, timestamp));
                                yield strength == 99 ? -1 : strength;
                            }
                            default -> null;
                        };
                    });
            installer.install(registry.getDeclaredMethod("listenWithEventList", boolean.class,
                                      boolean.class, int.class, String.class, String.class,
                                      listener, int[].class, boolean.class),
                    call -> {
                        call.arguments[6] =
                                adapter.wrap(call.arguments[0], (String) call.arguments[4],
                                        (int) call.arguments[3], call.arguments[6]);
                        Object result = call.original();
                        adapter.track(call.arguments[0]);
                        return result;
                    });
            Log.i(TAG, "Cell listener hooks installed");
            return adapter;
        } catch (Exception error) {
            Log.w(TAG, "Cell listeners unavailable", error);
            return null;
        }
    }
}
