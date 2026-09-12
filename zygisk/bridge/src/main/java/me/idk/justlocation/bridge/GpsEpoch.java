package me.idk.justlocation.bridge;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/** One coherent receiver epoch, shared by all applications and all GNSS channels. */
final class GpsEpoch {
    record Observation(GpsOrbit.Ephemeris orbit, float elevation, float azimuth, float cn0,
            long transmitNanos, double rangeRate) {}
    final long gpsMillis;
    final long gpsNanos;
    final List<Observation> observations;
    GpsEpoch(double lat, double lon, double altitude, double speed, double bearing,
            long timestampMs) {
        this(lat, lon, altitude, speed, bearing, timestampMs,
                GpsOrbit.gpsMillis(timestampMs) * 1000000L);
    }
    GpsEpoch(double lat, double lon, double altitude, double speed, double bearing,
            long timestampMs, long gpsNanos) {
        this.gpsNanos = gpsNanos;
        gpsMillis = Math.floorDiv(gpsNanos, 1000000);
        double seconds = gpsMillis / 1000.0;
        long towNanos = Math.floorMod(gpsNanos, GpsOrbit.WEEK * 1000000000L);
        double tow = towNanos / 1e9;
        var receiver = GpsOrbit.receiver(lat, lon, altitude);
        var velocity = GpsOrbit.velocity(lat, lon, speed, bearing);
        double phi = Math.toRadians(lat), lambda = Math.toRadians(lon);
        List<Observation> all = new ArrayList<>();
        for (int id = 1; id <= 32; id++) {
            var e = GpsOrbit.ephemeris(id, seconds);
            double range = GpsOrbit.range(e, tow, receiver);
            var delta = GpsOrbit.received(e, tow, range / GpsOrbit.C).minus(receiver);
            double east = -Math.sin(lambda) * delta.x() + Math.cos(lambda) * delta.y();
            double north = -Math.sin(phi) * Math.cos(lambda) * delta.x()
                    - Math.sin(phi) * Math.sin(lambda) * delta.y() + Math.cos(phi) * delta.z();
            double up = Math.cos(phi) * Math.cos(lambda) * delta.x()
                    + Math.cos(phi) * Math.sin(lambda) * delta.y() + Math.sin(phi) * delta.z();
            float elevation = (float) Math.toDegrees(Math.atan2(up, Math.hypot(east, north)));
            float azimuth = (float) ((Math.toDegrees(Math.atan2(east, north)) + 360) % 360);
            double rate =
                    (GpsOrbit.range(e, tow + .05, receiver.plus(velocity.scale(.05)))
                            - GpsOrbit.range(e, tow - .05, receiver.minus(velocity.scale(.05))))
                    / .1;
            long tx = Math.floorMod(
                    towNanos - Math.round(range / GpsOrbit.C * 1e9), GpsOrbit.WEEK * 1000000000L);
            all.add(new Observation(e, elevation, azimuth,
                    30 + 15 * (float) Math.max(0, Math.sin(Math.toRadians(elevation))), tx, rate));
        }
        all.sort(Comparator.comparingDouble(Observation::elevation).reversed());
        observations = List.copyOf(all.subList(0, 8));
    }
    /** Completed 6-second LNAV subframe, independent of registration count or callback rate. */
    long navigationSlot() {
        return Math.floorDiv(gpsMillis, 6000) - 1;
    }
}
