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
    private static final List<Satellite> SATELLITES =
            List.of(new Satellite(3, 38, 24, 35), new Satellite(7, 42, 61, 85),
                    new Satellite(11, 35, 18, 135), new Satellite(14, 44, 72, 180),
                    new Satellite(19, 40, 46, 225), new Satellite(22, 37, 31, 270),
                    new Satellite(26, 41, 55, 310), new Satellite(30, 36, 22, 350));
    private static final DateTimeFormatter TIME =
            DateTimeFormatter.ofPattern("HHmmss.SSS", Locale.ROOT).withZone(ZoneOffset.UTC);
    private static final DateTimeFormatter DATE =
            DateTimeFormatter.ofPattern("ddMMyy", Locale.ROOT).withZone(ZoneOffset.UTC);
    final long timestampMs;
    private final double latitude, longitude, altitude, speed, bearing;

    GnssFrame(double latitude, double longitude, double altitude, double speed, double bearing,
            long timestampMs) {
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
    }

    List<Satellite> satellites() {
        return SATELLITES;
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
        // Without a geoid model, use a documented synthetic zero separation. Thus ellipsoid
        // altitude = MSL altitude + separation remains consistent with Location.getAltitude().
        result.add(
                sentence(format("GPGGA,%s,%s,1,08,1.0,%.3f,M,0.000,M,,", time, point, altitude)));
        result.add(sentence(format("GPRMC,%s,A,%s,%.3f,%.3f,%s,,,A", time, point,
                speed / 0.5144444444444445, bearing, DATE.format(instant))));
        result.add(sentence("GPGSA,A,3,03,07,11,14,19,22,26,30,,,,,1.5,1.0,1.1"));
        for (int page = 0; page < 2; page++) {
            StringBuilder body = new StringBuilder("GPGSV,2," + (page + 1) + ",08");
            for (Satellite satellite : SATELLITES.subList(page * 4, page * 4 + 4)) {
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
