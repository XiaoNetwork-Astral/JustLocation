package me.idk.justlocation.bridge;

import android.location.GnssStatus;

import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Build raw measurements and navigation messages corresponding to the satellite table.
 * Pseudoranges and LNAV use the same quantized, offline GPS orbit model. Constructors
 * and setters are resolved at runtime because SDK stubs hide them. Missing required APIs disable
 * the raw channels.
 */
final class GnssRawOutput {
    private static final String CLOCK_TYPE = "android.location.GnssClock";
    private static final String MEASUREMENT_TYPE = "android.location.GnssMeasurement";
    private static final String EVENT_TYPE = "android.location.GnssMeasurementsEvent";
    private static final String MESSAGE_TYPE = "android.location.GnssNavigationMessage";

    /** GPS L1 carrier frequency. */
    private static final float L1_HZ = 1575.42e6f;

    private static final Map<String, Method> SETTERS = new HashMap<>();
    private static final List<String> MISSING = new ArrayList<>();
    private static final Map<String, Integer> CONSTANTS = new HashMap<>();
    private static Boolean verified;

    private static final Class<?> CLOCK = load(CLOCK_TYPE);
    private static final Class<?> MEASUREMENT = load(MEASUREMENT_TYPE);
    private static final Class<?> EVENT = load(EVENT_TYPE);
    private static final Class<?> MESSAGE = load(MESSAGE_TYPE);
    private static final Class<?> BUILDER = load(EVENT_TYPE + "$Builder");

    private static final Constructor<?> NEW_CLOCK = constructor(CLOCK);
    private static final Constructor<?> NEW_MEASUREMENT = constructor(MEASUREMENT);
    private static final Constructor<?> NEW_MESSAGE = constructor(MESSAGE);
    private static final Constructor<?> NEW_EVENT_BUILDER = constructor(BUILDER);

    private static final Method EVENT_BUILD = optional(BUILDER, "build");
    private static final Method EVENT_SET_CLOCK = optional(BUILDER, "setClock", CLOCK);
    private static final Method EVENT_SET_MEASUREMENTS =
            optional(BUILDER, "setMeasurements", java.util.Collection.class);
    private static final Method EVENT_SET_FULL_TRACKING =
            optional(BUILDER, "setIsFullTracking", boolean.class);

    /** Setter name and parameter type for table-driven assignment. */
    private record Assignment(String name, Class<?> type) {}

    private static final Assignment[] CLOCK_FIELDS = {
            new Assignment("setTimeNanos", long.class),
            new Assignment("setTimeUncertaintyNanos", double.class),
            new Assignment("setFullBiasNanos", long.class),
            new Assignment("setBiasNanos", double.class),
            new Assignment("setBiasUncertaintyNanos", double.class),
            new Assignment("setDriftNanosPerSecond", double.class),
            new Assignment("setDriftUncertaintyNanosPerSecond", double.class),
            new Assignment("setElapsedRealtimeNanos", long.class),
            new Assignment("setElapsedRealtimeUncertaintyNanos", double.class),
            new Assignment("setLeapSecond", int.class),
            new Assignment("setHardwareClockDiscontinuityCount", int.class),
    };
    private static final Assignment[] MEASUREMENT_FIELDS = {
            new Assignment("setSvid", int.class),
            new Assignment("setConstellationType", int.class),
            new Assignment("setTimeOffsetNanos", double.class),
            new Assignment("setState", int.class),
            new Assignment("setReceivedSvTimeNanos", long.class),
            new Assignment("setReceivedSvTimeUncertaintyNanos", long.class),
            new Assignment("setCn0DbHz", double.class),
            new Assignment("setBasebandCn0DbHz", double.class),
            new Assignment("setPseudorangeRateMetersPerSecond", double.class),
            new Assignment("setPseudorangeRateUncertaintyMetersPerSecond", double.class),
            new Assignment("setCarrierFrequencyHz", float.class),
            new Assignment("setMultipathIndicator", int.class),
            new Assignment("setSnrInDb", double.class),
            new Assignment("setAutomaticGainControlLevelInDb", double.class),
    };
    private static final Assignment[] MESSAGE_FIELDS = {
            new Assignment("setType", int.class),
            new Assignment("setSvid", int.class),
            new Assignment("setMessageId", int.class),
            new Assignment("setSubmessageId", int.class),
            new Assignment("setStatus", int.class),
            new Assignment("setData", byte[].class),
    };
    private static final String[] MEASUREMENT_CONSTANTS = {
            "STATE_CODE_LOCK",
            "STATE_SYMBOL_SYNC",
            "STATE_SUBFRAME_SYNC",
            "STATE_TOW_DECODED",
            "MULTIPATH_INDICATOR_NOT_DETECTED",
    };
    private static final String[] MESSAGE_CONSTANTS = {"TYPE_GPS_L1CA", "STATUS_PARITY_PASSED"};

    static {
        // Resolve required setters and constants before reporting usability.
        for (Assignment assignment : CLOCK_FIELDS)
            setter(CLOCK, assignment.name(), assignment.type());
        for (Assignment assignment : MEASUREMENT_FIELDS)
            setter(MEASUREMENT, assignment.name(), assignment.type());
        for (Assignment assignment : MESSAGE_FIELDS)
            setter(MESSAGE, assignment.name(), assignment.type());
        for (String name : MEASUREMENT_CONSTANTS)
            constant(MEASUREMENT, name);
        for (String name : MESSAGE_CONSTANTS)
            constant(MESSAGE, name);
    }

    /** Whether all APIs required by both raw channels are available. */
    static synchronized boolean usable() {
        if (verified != null)
            return verified;
        boolean resolved = MISSING.isEmpty() && NEW_CLOCK != null && NEW_MEASUREMENT != null
                && NEW_MESSAGE != null && NEW_EVENT_BUILDER != null && EVENT_BUILD != null
                && EVENT_SET_CLOCK != null && EVENT_SET_MEASUREMENTS != null;
        if (resolved) {
            try {
                var sample = new GnssFrame(0, 0, 0, 0, 0, System.currentTimeMillis());
                measurements(sample);
                navigationMessages(sample);
            } catch (RuntimeException failure) {
                MISSING.add("object construction: " + failure);
                resolved = false;
            }
        }
        verified = resolved;
        return resolved;
    }

    /** Missing APIs for installation diagnostics. */
    static String missing() {
        return String.join(", ", MISSING);
    }

    private GnssRawOutput() {}

    /** One receiver clock and one measurement per simulated satellite. */
    static Object measurements(GnssFrame frame) {
        long now = frame.elapsedNanos;
        long gpsNanos = frame.gps.gpsNanos;
        Object clock = build(NEW_CLOCK, CLOCK, CLOCK_FIELDS, now, 1.0, now - gpsNanos, 0.0, 1.0,
                0.0, 0.0, now, 1_000_000.0, GpsOrbit.LEAP_SECONDS, frame.discontinuities);

        int state = constant(MEASUREMENT, "STATE_CODE_LOCK")
                | constant(MEASUREMENT, "STATE_SYMBOL_SYNC")
                | constant(MEASUREMENT, "STATE_SUBFRAME_SYNC")
                | constant(MEASUREMENT, "STATE_TOW_DECODED");
        int clean = constant(MEASUREMENT, "MULTIPATH_INDICATOR_NOT_DETECTED");
        List<Object> measurements = new ArrayList<>(frame.satellites().size());
        for (GpsEpoch.Observation satellite : frame.gps.observations) {
            measurements.add(build(NEW_MEASUREMENT, MEASUREMENT, MEASUREMENT_FIELDS,
                    satellite.orbit().id(), GnssStatus.CONSTELLATION_GPS, 0.0, state,
                    satellite.transmitNanos(), 1L, (double) satellite.cn0(),
                    (double) satellite.cn0() - 1.5, satellite.rangeRate(), 0.05, L1_HZ, clean,
                    (double) satellite.cn0() - 20.0, -6.0));
        }
        return event(clock, measurements);
    }

    /**
     * Generate one GPS L1 C/A subframe per satellite, cycling subframe IDs from 1 to 5. The caller
     * delivers the batch one message at a time.
     */
    static List<Object> navigationMessages(GnssFrame frame) {
        int type = constant(MESSAGE, "TYPE_GPS_L1CA");
        int passed = constant(MESSAGE, "STATUS_PARITY_PASSED");
        List<Object> messages = new ArrayList<>(frame.satellites().size());
        for (GnssFrame.Satellite satellite : frame.satellites()) {
            var nav = GpsLnav.message(satellite.id(), frame.gps.navigationSlot());
            messages.add(build(NEW_MESSAGE, MESSAGE, MESSAGE_FIELDS, type, satellite.id(),
                    nav.page(), nav.subframe(), passed, nav.data()));
        }
        return messages;
    }

    /** The event requires its Builder API to combine clock and measurement objects. */
    private static Object event(Object clock, List<Object> measurements) {
        try {
            Object builder = NEW_EVENT_BUILDER.newInstance();
            EVENT_SET_CLOCK.invoke(builder, clock);
            EVENT_SET_MEASUREMENTS.invoke(builder, measurements);
            if (EVENT_SET_FULL_TRACKING != null)
                EVENT_SET_FULL_TRACKING.invoke(builder, true);
            return EVENT_BUILD.invoke(builder);
        } catch (ReflectiveOperationException error) {
            throw new IllegalStateException("Cannot build GnssMeasurementsEvent", error);
        }
    }

    private static Object build(
            Constructor<?> constructor, Class<?> owner, Assignment[] fields, Object... values) {
        try {
            Object target = constructor.newInstance();
            for (int index = 0; index < fields.length; index++) {
                Method setter = setter(owner, fields[index].name(), fields[index].type());
                if (setter != null)
                    setter.invoke(target, values[index]);
            }
            return target;
        } catch (ReflectiveOperationException error) {
            throw new IllegalStateException("Cannot build " + owner.getSimpleName(), error);
        }
    }

    // Reflection helpers.

    private static Class<?> load(String name) {
        try {
            return Class.forName(name, false, GnssRawOutput.class.getClassLoader());
        } catch (Throwable error) {
            MISSING.add(name);
            return null;
        }
    }

    private static Constructor<?> constructor(Class<?> type) {
        if (type == null)
            return null;
        try {
            Constructor<?> constructor = type.getDeclaredConstructor();
            constructor.setAccessible(true);
            return constructor;
        } catch (Throwable error) {
            MISSING.add(type.getSimpleName() + "()");
            return null;
        }
    }

    private static Method optional(Class<?> type, String name, Class<?>... parameters) {
        if (type == null)
            return null;
        try {
            Method method = type.getMethod(name, parameters);
            method.setAccessible(true);
            return method;
        } catch (Throwable error) {
            MISSING.add(type.getSimpleName() + '.' + name);
            return null;
        }
    }

    private static Method setter(Class<?> owner, String name, Class<?> type) {
        if (owner == null)
            return null;
        String key = owner.getName() + '/' + name + '/' + type.getName();
        Method method = SETTERS.get(key);
        if (method != null)
            return method;
        try {
            method = owner.getMethod(name, type);
            method.setAccessible(true);
            SETTERS.put(key, method);
            return method;
        } catch (Throwable error) {
            String miss = owner.getSimpleName() + '.' + name;
            if (!MISSING.contains(miss))
                MISSING.add(miss);
            return null;
        }
    }

    private static int constant(Class<?> owner, String name) {
        Integer value = CONSTANTS.get(name);
        if (value != null)
            return value;
        try {
            int resolved = owner.getField(name).getInt(null);
            CONSTANTS.put(name, resolved);
            return resolved;
        } catch (Throwable error) {
            String miss = owner.getSimpleName() + '.' + name;
            if (!MISSING.contains(miss))
                MISSING.add(miss);
            return 0;
        }
    }
}
