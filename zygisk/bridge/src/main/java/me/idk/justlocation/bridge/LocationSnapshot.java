package me.idk.justlocation.bridge;

import android.location.Location;
import android.location.LocationManager;
import android.os.Bundle;
import android.os.SystemClock;

import org.json.JSONObject;

/** Position, scope and satellite flags from one valid heartbeat. */
final class LocationSnapshot {
    final SessionSnapshot scope;
    final double latitude, longitude, altitude;
    final float accuracy, speed, bearing;
    /** Missing satellite settings default to disabled. */
    final boolean gnssEnabled, nmeaEnabled;
    /**
     * Wall-clock time the backend computed this position, or null for a heartbeat that predates the
     * field. Every read of one heartbeat reports the same sample time, so reading a position again
     * cannot make it look newly sampled.
     */
    final Long sampledMs;
    LocationSnapshot(SessionSnapshot scope, JSONObject position, boolean gnssEnabled,
            boolean nmeaEnabled, Long sampledMs) throws Exception {
        this.scope = scope;
        this.gnssEnabled = gnssEnabled;
        this.nmeaEnabled = nmeaEnabled;
        this.sampledMs = sampledMs;
        latitude = position.getDouble("latitude");
        longitude = position.getDouble("longitude");
        altitude = position.getDouble("altitude");
        accuracy = (float) position.getDouble("accuracy");
        speed = (float) position.getDouble("speed");
        bearing = (float) position.getDouble("bearing");
        if (!Double.isFinite(latitude) || Math.abs(latitude) > 90 || !Double.isFinite(longitude)
                || Math.abs(longitude) > 180 || !Double.isFinite(altitude)
                || !Float.isFinite(accuracy) || accuracy < 0 || !Float.isFinite(speed) || speed < 0
                || !Float.isFinite(bearing) || bearing < 0 || bearing >= 360) {
            throw new IllegalArgumentException("Invalid backend position");
        }
    }
    static LocationSnapshot parse(String response) throws Exception {
        JSONObject json = new JSONObject(response);
        if (json.getInt("version") != 1 || !json.getBoolean("ok"))
            return null;
        JSONObject state = json.getJSONObject("state");
        if (!state.getBoolean("requested_active"))
            return null;
        JSONObject config = state.getJSONObject("config");
        ScopeSelection selection = ScopeSelection.read(state, null);
        if (selection == null)
            return null;
        // Missing or malformed satellite flags leave system output unchanged.
        boolean gnssEnabled = false, nmeaEnabled = false;
        JSONObject gnss = state.optJSONObject("gnss");
        if (gnss != null) {
            gnssEnabled = gnss.optBoolean("gnss_enabled", false);
            nmeaEnabled = gnss.optBoolean("nmea_enabled", false);
        }
        // A daemon without the field reports a fix time as unknown, never as "now".
        Long sampledMs =
                state.has("position_sampled_ms") && !state.isNull("position_sampled_ms")
                        ? state.getLong("position_sampled_ms")
                        : null;
        if (sampledMs != null && sampledMs <= 0)
            sampledMs = null;
        return new LocationSnapshot(selection.snapshot(SystemClock.elapsedRealtime()),
                config.getJSONObject("position"), gnssEnabled, nmeaEnabled, sampledMs);
    }

    /**
     * The fix time this heartbeat reports. Without a backend sample time, the fix is stamped when
     * the heartbeat was received rather than when it happens to be read.
     */
    long fixTimeMs() {
        return fixTime(sampledMs, System.currentTimeMillis());
    }

    /**
     * A sample time is reused as-is, while a timestamp outside the heartbeat window is replaced by
     * the read time: a position older than the snapshot that carries it cannot be delivered as a
     * current fix, and a clock step must not turn into a fix from the future.
     */
    static long fixTime(Long sampledMs, long nowMs) {
        if (sampledMs == null || sampledMs <= 0)
            return nowMs;
        long age = nowMs - sampledMs;
        return age >= 0 && age < SessionSnapshot.MAX_AGE_MS ? sampledMs : nowMs;
    }

    /**
     * Monotonic timestamp for the same sample. `Location.getElapsedRealtimeNanos` must stay
     * comparable with `SystemClock.elapsedRealtimeNanos`, so the age of the sample is subtracted
     * from the current monotonic clock.
     */
    static long fixElapsedNanos(long sampledMs, long nowMs, long elapsedNanos) {
        return elapsedNanos + (sampledMs - nowMs) * 1_000_000L;
    }

    /**
     * Whether a fix from this provider carries a satellite count.
     *
     * Only a GPS fix from the running satellite model does. A network, fused or passive fix is not
     * a satellite solution, so it carries no count at all instead of borrowing the GPS one; the
     * same absence applies while the satellite model is off, where the platform itself reports no
     * satellites either.
     */
    static boolean carriesSatellites(String provider, boolean gnssEnabled) {
        return gnssEnabled && provider != null
                && provider.equalsIgnoreCase(LocationManager.GPS_PROVIDER);
    }

    /** Modeled satellites used in the fix, shared with NMEA and GnssStatus. */
    static int satelliteCount(double latitude, double longitude, double altitude, double speed,
            double bearing, long timestampMs) {
        return new GpsEpoch(latitude, longitude, altitude, speed, bearing, timestampMs)
                .observations.size();
    }

    Location location(String provider) {
        Location result = new Location(provider);
        result.setLatitude(latitude);
        result.setLongitude(longitude);
        result.setAltitude(altitude);
        result.setAccuracy(accuracy);
        result.setSpeed(speed);
        result.setBearing(bearing);
        // Both timestamps describe the sample, not the read: a consumer that asks again sees the
        // same fix age instead of a fix that was just produced by their own query.
        long sampled = fixTimeMs();
        long now = System.currentTimeMillis();
        result.setTime(sampled);
        result.setElapsedRealtimeNanos(
                fixElapsedNanos(sampled, now, SystemClock.elapsedRealtimeNanos()));
        if (carriesSatellites(provider, gnssEnabled))
            extras(result, satelliteCount(latitude, longitude, altitude, speed, bearing, sampled));
        return result;
    }

    /**
     * Android 15 returns a new Bundle from `Location.getExtras()` until one is supplied, so the
     * satellite extra is set through a fresh bundle and an explicit setter.
     */
    private static void extras(Location location, int satellites) {
        Bundle bundle = new Bundle();
        bundle.putInt("satellites", satellites);
        location.setExtras(bundle);
    }
}
