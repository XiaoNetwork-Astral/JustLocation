package me.idk.justlocation.bridge;

import java.time.Instant;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/** Position and time shared by satellite status, NMEA and raw GNSS output. */
final class GnssFrame {
    record Satellite(int id, float cn0, float elevation, float azimuth) {}
    final GpsEpoch gps;
    private final List<Satellite> satellites;
    private static final DateTimeFormatter TIME =
            DateTimeFormatter.ofPattern("HHmmss.SSS", Locale.ROOT).withZone(ZoneOffset.UTC);
    private static final DateTimeFormatter DATE =
            DateTimeFormatter.ofPattern("ddMMyy", Locale.ROOT).withZone(ZoneOffset.UTC);
    final long timestampMs;
    final long elapsedNanos;
    final int discontinuities;
    private final double latitude, longitude, altitude, speed, bearing;

    GnssFrame(double latitude, double longitude, double altitude, double speed, double bearing,
            long timestampMs) {
        this(latitude, longitude, altitude, speed, bearing, timestampMs,
                GpsOrbit.gpsMillis(timestampMs) * 1000000L, 0, 0);
    }
    GnssFrame(double latitude, double longitude, double altitude, double speed, double bearing,
            long timestampMs, long gpsNanos, long elapsedNanos, int discontinuities) {
        if (!Double.isFinite(latitude) || Math.abs(latitude) > 90 || !Double.isFinite(longitude)
                || Math.abs(longitude) > 180 || !Double.isFinite(altitude)
                || !Double.isFinite(speed) || speed < 0 || !Double.isFinite(bearing) || bearing < 0
                || bearing >= 360) {
            throw new IllegalArgumentException("Invalid GNSS position");
        }
        this.latitude = latitude;
        this.longitude = longitude;
        this.altitude = altitude;
        this.speed = speed;
        this.bearing = bearing;
        this.timestampMs = timestampMs;
        this.elapsedNanos = elapsedNanos;
        this.discontinuities = discontinuities;
        gps = new GpsEpoch(latitude, longitude, altitude, speed, bearing, timestampMs, gpsNanos);
        satellites = gps.observations.stream()
                             .map(s
                                     -> new Satellite(
                                             s.orbit().id(), s.cn0(), s.elevation(), s.azimuth()))
                             .toList();
    }

    List<Satellite> satellites() {
        return satellites;
    }

    /** Read-only speed for deriving the pseudorange rate. */
    double speed() {
        return speed;
    }

    List<String> nmea() {
        Instant instant = Instant.ofEpochMilli(timestampMs);
        String time = TIME.format(instant);
        String point = coordinate(latitude, 2) + "," + (latitude < 0 ? "S" : "N") + ","
                + coordinate(longitude, 3) + "," + (longitude < 0 ? "W" : "E");
        ArrayList<String> result = new ArrayList<>();
        // One epoch defines the satellite count for every channel, so NMEA cannot claim more
        // satellites than the model solved with.
        int used = satellites.size();
        int pages = (used + 3) / 4;
        // Without a geoid model, use a documented synthetic zero separation. Thus ellipsoid
        // altitude = MSL altitude + separation remains consistent with Location.getAltitude().
        result.add(sentence(format("GPGGA,%s,%s,1,%02d,1.0,%.3f,M,0.000,M,,", time, point, used,
                altitude)));
        result.add(sentence(format("GPRMC,%s,A,%s,%.3f,%.3f,%s,,,A", time, point,
                speed / 0.5144444444444445, bearing, DATE.format(instant))));
        StringBuilder gsa = new StringBuilder("GPGSA,A,3");
        for (Satellite satellite : satellites)
            gsa.append(format(",%02d", satellite.id()));
        result.add(sentence(gsa + ",,,,,1.5,1.0,1.1"));
        for (int page = 0; page < pages; page++) {
            int first = page * 4;
            int last = Math.min(first + 4, used);
            StringBuilder body =
                    new StringBuilder(format("GPGSV,%d,%d,%02d", pages, page + 1, used));
            for (Satellite satellite : satellites.subList(first, last)) {
                body.append(format(",%02d,%02.0f,%03.0f,%02.0f", satellite.id, satellite.elevation,
                        satellite.azimuth, satellite.cn0));
            }
            result.add(sentence(body.toString()));
        }
        result.add(sentence(format("GPVTG,%.3f,T,,M,%.3f,N,%.3f,K,A", bearing,
                speed / 0.5144444444444445, speed * 3.6)));
        return List.copyOf(result);
    }

    private static String coordinate(double value, int degreeDigits) {
        long units = Math.round(Math.abs(value) * 60 * 100000);
        long degrees = units / 6000000, minutes = units % 6000000;
        return format(
                "%0" + degreeDigits + "d%02d.%05d", degrees, minutes / 100000, minutes % 100000);
    }
    private static String format(String pattern, Object... values) {
        return String.format(Locale.ROOT, pattern, values);
    }
    private static String sentence(String body) {
        int checksum = 0;
        for (int i = 0; i < body.length(); i++)
            checksum ^= body.charAt(i);
        return "$" + body + format("*%02X\r\n", checksum);
    }
}
