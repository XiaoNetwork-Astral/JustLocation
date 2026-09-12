package me.idk.justlocation.bridge;

import android.location.Location;
import android.os.SystemClock;

import java.util.HashSet;
import java.util.Set;

import org.json.JSONObject;

/** Position, scope and satellite flags from one valid heartbeat. */
final class LocationSnapshot {
    final SessionSnapshot scope;
    final double latitude, longitude, altitude;
    final float accuracy, speed, bearing;
    /** Missing satellite settings default to disabled. */
    final boolean gnssEnabled, nmeaEnabled;
    LocationSnapshot(SessionSnapshot scope, JSONObject position, boolean gnssEnabled,
            boolean nmeaEnabled) throws Exception {
        this.scope = scope;
        this.gnssEnabled = gnssEnabled;
        this.nmeaEnabled = nmeaEnabled;
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
        JSONObject selection = config.getJSONObject("scope");
        String mode = selection.getString("mode");
        Set<String> packages = new HashSet<>();
        if (mode.equals("apps")) {
            var list = selection.getJSONArray("packages");
            for (int i = 0; i < list.length(); i++)
                packages.add(list.getString(i));
        } else if (!mode.equals("all"))
            return null;
        // Missing or malformed satellite flags leave system output unchanged.
        boolean gnssEnabled = false, nmeaEnabled = false;
        JSONObject gnss = state.optJSONObject("gnss");
        if (gnss != null) {
            gnssEnabled = gnss.optBoolean("gnss_enabled", false);
            nmeaEnabled = gnss.optBoolean("nmea_enabled", false);
        }
        return new LocationSnapshot(new SessionSnapshot(true, mode.equals("all"), packages,
                                            SystemClock.elapsedRealtime()),
                config.getJSONObject("position"), gnssEnabled, nmeaEnabled);
    }
    Location location(String provider) {
        Location result = new Location(provider);
        result.setLatitude(latitude);
        result.setLongitude(longitude);
        result.setAltitude(altitude);
        result.setAccuracy(accuracy);
        result.setSpeed(speed);
        result.setBearing(bearing);
        result.setTime(System.currentTimeMillis());
        result.setElapsedRealtimeNanos(SystemClock.elapsedRealtimeNanos());
        return result;
    }
}
