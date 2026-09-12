package me.idk.justlocation.bridge;

import android.location.GnssStatus;
import android.os.SystemClock;
import android.util.Log;

import java.util.ArrayList;
import java.util.List;

/** Install satellite, NMEA, measurement and navigation hooks and publish their output frame. */
final class GnssHooks {
    private static final String TAG = "JustLocation";
    private final MethodHook.Installer installer;
    private volatile GnssListener.Output output;
    private volatile GnssRawListener.Output rawOutput;
    final List<GnssDispatcher> dispatchers = new ArrayList<>();
    int flags;
    private long clockBias;
    private boolean clockStarted;
    private int clockDiscontinuities;

    GnssHooks(MethodHook.Installer installer) {
        this.installer = installer;
    }

    void install(ClassLoader loader) {
        installGnss(loader, "GnssStatusProvider", "IGnssStatusListener", 2);
        installGnss(loader, "GnssNmeaProvider", "IGnssNmeaListener", 4);
        installGnssRaw(loader);
    }

    void update(LocationSnapshot current) {
        long elapsed = SystemClock.elapsedRealtimeNanos(), wall = System.currentTimeMillis();
        long observed = GpsOrbit.gpsMillis(wall) * 1000000L;
        if (!clockStarted || Math.abs((elapsed - clockBias) - observed) > 1_000_000_000L) {
            if (clockStarted)
                clockDiscontinuities++;
            clockBias = elapsed - observed;
            clockStarted = true;
        }
        long gpsNanos = elapsed - clockBias;
        long timestamp = Math.floorDiv(gpsNanos, 1000000) + GpsOrbit.UNIX_EPOCH_MS
                - GpsOrbit.LEAP_SECONDS * 1000L;
        GnssFrame frame = current == null
                ? null
                : new GnssFrame(current.latitude, current.longitude, current.altitude,
                          current.speed, current.bearing, timestamp, gpsNanos, elapsed,
                          clockDiscontinuities);
        // All GNSS channels and NMEA share this frame.
        output = frame == null ? null
                               : new GnssListener.Output(current.scope, frame, current.gnssEnabled,
                                         current.nmeaEnabled);
        rawOutput = frame == null
                ? null
                : new GnssRawListener.Output(current.scope, frame, current.gnssEnabled);
    }

    private void installGnss(
            ClassLoader loader, String providerName, String listenerName, int flag) {
        try {
            Class<?> provider = Class.forName(
                    "com.android.server.location.gnss." + providerName, false, loader);
            Class<?> listener = Class.forName("android.location." + listenerName, false, loader);
            Class<?> identity =
                    Class.forName("android.location.util.identity.CallerIdentity", false, loader);
            Class<?> registration = Class.forName(
                    "com.android.server.location.gnss.GnssListenerMultiplexer$GnssListenerRegistration",
                    false, loader);
            Class<?> operation = Class.forName(
                    "com.android.internal.listeners.ListenerExecutor$ListenerOperation", false,
                    loader);
            GnssDispatcher channel =
                    new GnssDispatcher(provider, listener, registration, identity, operation,
                            (delegate, packageName)
                                    -> new GnssListener(listener, delegate, packageName,
                                            ()
                                                    -> output,
                                            SystemClock::elapsedRealtime, GnssHooks::gnssStatus)
                                            .proxy());
            installer.install(
                    provider.getDeclaredMethod("addListener", identity, listener), call -> {
                        call.arguments[2] = channel.wrap(call.arguments[1], call.arguments[2]);
                        Object result = call.original();
                        channel.track(call.arguments[0]);
                        return result;
                    });
            dispatchers.add(channel);
            flags |= flag;
            Log.i(TAG, providerName + " hooks installed");
        } catch (Exception error) {
            Log.w(TAG, "Cannot install " + providerName + "; location channel remains available",
                    error);
        }
    }

    private void installGnssRaw(ClassLoader loader) {
        // Missing object APIs disable both raw channels.
        if (!GnssRawOutput.usable()) {
            Log.w(TAG,
                    "Raw GNSS object APIs missing (" + GnssRawOutput.missing()
                            + "); raw channels stay system output");
            return;
        }
        Class<?> listener, registration, identity, operation, request;
        try {
            identity =
                    Class.forName("android.location.util.identity.CallerIdentity", false, loader);
            registration = Class.forName(
                    "com.android.server.location.gnss.GnssListenerMultiplexer$GnssListenerRegistration",
                    false, loader);
            operation = Class.forName(
                    "com.android.internal.listeners.ListenerExecutor$ListenerOperation", false,
                    loader);
            request = Class.forName("android.location.GnssMeasurementRequest", false, loader);
        } catch (Exception error) {
            Log.w(TAG, "Raw GNSS support classes unavailable", error);
            return;
        }
        boolean measurements =
                installGnssMeasurement(loader, identity, registration, operation, request);
        boolean messages = installGnssMessage(loader, identity, registration, operation);
        if (measurements && messages)
            flags |= 64;
        Log.i(TAG,
                "Raw GNSS hooks: measurements=" + measurements + " navigationMessages=" + messages);
    }

    String counters() {
        StringBuilder counters = new StringBuilder();
        for (int index = 0; index < dispatchers.size(); index++) {
            if (counters.length() > 0)
                counters.append(';');
            counters.append(index).append(':').append(
                    String.join(",", dispatchers.get(index).counters()));
        }
        return counters.toString();
    }

    private boolean installGnssMeasurement(ClassLoader loader, Class<?> identity,
            Class<?> registration, Class<?> operation, Class<?> request) {
        try {
            Class<?> provider = Class.forName(
                    "com.android.server.location.gnss.GnssMeasurementsProvider", false, loader);
            Class<?> listener =
                    Class.forName("android.location.IGnssMeasurementsListener", false, loader);
            GnssDispatcher channel = new GnssDispatcher(provider, listener, registration, identity,
                    operation,
                    (delegate, packageName)
                            -> new GnssRawListener(listener, delegate, packageName,
                                    ()
                                            -> rawOutput,
                                    SystemClock::elapsedRealtime, "onGnssMeasurementsReceived",
                                    frame -> java.util.List.of(GnssRawOutput.measurements(frame)))
                                    .proxy(),
                    // Drive synthetic measurements without waiting for HAL callbacks.
                    true);
            installer.install(
                    provider.getDeclaredMethod("addListener", request, identity, listener),
                    call -> {
                        call.arguments[3] = channel.wrap(call.arguments[2], call.arguments[3]);
                        Object result = call.original();
                        channel.track(call.arguments[0]);
                        return result;
                    });
            dispatchers.add(channel);
            return true;
        } catch (Throwable error) {
            Log.w(TAG, "Cannot install raw GNSS measurements; that channel stays system output",
                    error);
            return false;
        }
    }

    private boolean installGnssMessage(
            ClassLoader loader, Class<?> identity, Class<?> registration, Class<?> operation) {
        try {
            Class<?> provider =
                    Class.forName("com.android.server.location.gnss.GnssNavigationMessageProvider",
                            false, loader);
            Class<?> listener =
                    Class.forName("android.location.IGnssNavigationMessageListener", false, loader);
            GnssDispatcher channel = new GnssDispatcher(provider, listener, registration, identity,
                    operation,
                    (delegate, packageName)
                            -> new GnssRawListener(listener, delegate, packageName,
                                    ()
                                            -> rawOutput,
                                    SystemClock::elapsedRealtime, "onGnssNavigationMessageReceived",
                                    frame
                                    -> new ArrayList<Object>(
                                            GnssRawOutput.navigationMessages(frame)))
                                    .proxy(),
                    // Drive navigation messages without waiting for HAL callbacks.
                    true);
            installer.install(
                    provider.getDeclaredMethod("addListener", identity, listener), call -> {
                        call.arguments[2] = channel.wrap(call.arguments[1], call.arguments[2]);
                        Object result = call.original();
                        channel.track(call.arguments[0]);
                        return result;
                    });
            dispatchers.add(channel);
            return true;
        } catch (Throwable error) {
            Log.w(TAG, "Cannot install GNSS navigation messages; that channel stays system output",
                    error);
            return false;
        }
    }

    private static Object gnssStatus(GnssFrame frame) {
        GnssStatus.Builder builder = new GnssStatus.Builder();
        for (GnssFrame.Satellite satellite : frame.satellites()) {
            builder.addSatellite(GnssStatus.CONSTELLATION_GPS, satellite.id(), satellite.cn0(),
                    satellite.elevation(), satellite.azimuth(), true, true, true, true, 1575420000f,
                    false, 0);
        }
        return builder.build();
    }
}
