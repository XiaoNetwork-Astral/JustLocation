package me.idk.justlocation.bridge;

/**
 * Offline circular GPS constellation. Every orbit parameter is quantized before use on air
 * and in measurements (IS-GPS-200N, 20.3.3.4). Zero clock/atmosphere errors are intentional.
 */
final class GpsOrbit {
    static final double C = 299792458.0, MU = 3.986005e14, EARTH_RATE = 7.2921151467e-5;
    static final long WEEK = 604800, UNIX_EPOCH_MS = 315964800000L;
    static final int LEAP_SECONDS = 18;
    static final long SQRT_A = Math.round(Math.sqrt(26560000.0) * (1L << 19));
    static final int INCLINATION = angle(Math.toRadians(55));
    // Rounded nodal regression is used by both the encoder and propagator.
    static final int NODE_RATE = (int) Math.round(-8e-9 / Math.PI * (1L << 43));
    record Vector(double x, double y, double z) {
        Vector minus(Vector b) {
            return new Vector(x - b.x, y - b.y, z - b.z);
        }
        Vector plus(Vector b) {
            return new Vector(x + b.x, y + b.y, z + b.z);
        }
        Vector scale(double a) {
            return new Vector(x * a, y * a, z * a);
        }
        double norm() {
            return Math.sqrt(x * x + y * y + z * z);
        }
    }
    record Ephemeris(int id, long week, int toe, int issue, int mean, int node) {
        Vector position(double tow) {
            double dt = tow - toe;
            if (dt > WEEK / 2.0)
                dt -= WEEK;
            if (dt < -WEEK / 2.0)
                dt += WEEK;
            double a = Math.scalb((double) SQRT_A, -19);
            a *= a;
            double u = radians(mean) + Math.sqrt(MU / (a * a * a)) * dt;
            double omega = radians(node)
                    + (Math.scalb((double) NODE_RATE, -43) * Math.PI - EARTH_RATE) * dt
                    - EARTH_RATE * toe;
            double i = radians(INCLINATION), x = a * Math.cos(u), y = a * Math.sin(u);
            return new Vector(x * Math.cos(omega) - y * Math.cos(i) * Math.sin(omega),
                    x * Math.sin(omega) + y * Math.cos(i) * Math.cos(omega), y * Math.sin(i));
        }
    }
    static long gpsMillis(long unixMs) {
        return unixMs - UNIX_EPOCH_MS + LEAP_SECONDS * 1000L;
    }
    static int angle(double radians) {
        double circles = radians / Math.PI;
        circles -= 2 * Math.floor((circles + 1) / 2);
        return (int) Math.round(Math.scalb(circles, 31));
    }
    static double radians(int field) {
        return Math.scalb((double) field, -31) * Math.PI;
    }
    static Ephemeris ephemeris(int id, double gpsSeconds) {
        long week = (long) Math.floor(gpsSeconds / WEEK);
        int toe = (int) (Math.floor((gpsSeconds - week * WEEK) / 7200) * 7200);
        return at(id, week, toe);
    }
    static Ephemeris at(int id, long week, int toe) {
        if (id < 1 || id > 32)
            throw new IllegalArgumentException("GPS PRN outside 1..32");
        int plane = (id - 1) % 6, slot = (id - 1) / 6, slots = plane < 2 ? 6 : 5;
        double epoch = week * (double) WEEK + toe, a = Math.scalb((double) SQRT_A, -19);
        a *= a;
        double phase = 2 * Math.PI * slot / slots + plane * Math.PI / slots;
        double mean = phase + Math.sqrt(MU / (a * a * a)) * epoch;
        double node = plane * Math.PI / 3 + Math.scalb((double) NODE_RATE, -43) * Math.PI * epoch
                - EARTH_RATE * week * WEEK;
        int issue = (int) Math.floorMod((long) Math.floor(epoch / 7200), 240);
        return new Ephemeris(id, week, toe, issue, angle(mean), angle(node));
    }
    static Vector receiver(double latitude, double longitude, double altitude) {
        double lat = Math.toRadians(latitude), lon = Math.toRadians(longitude),
               e2 = 6.6943799901413165e-3;
        double n = 6378137 / Math.sqrt(1 - e2 * Math.sin(lat) * Math.sin(lat));
        return new Vector((n + altitude) * Math.cos(lat) * Math.cos(lon),
                (n + altitude) * Math.cos(lat) * Math.sin(lon),
                (n * (1 - e2) + altitude) * Math.sin(lat));
    }
    static Vector velocity(double latitude, double longitude, double speed, double bearing) {
        double lat = Math.toRadians(latitude), lon = Math.toRadians(longitude),
               b = Math.toRadians(bearing);
        double east = speed * Math.sin(b), north = speed * Math.cos(b);
        return new Vector(-Math.sin(lon) * east - Math.sin(lat) * Math.cos(lon) * north,
                Math.cos(lon) * east - Math.sin(lat) * Math.sin(lon) * north,
                Math.cos(lat) * north);
    }
    /** Satellite coordinates in the receiver's ECEF axes, including Earth rotation in flight. */
    static Vector received(Ephemeris e, double tow, double flight) {
        Vector p = e.position(tow - flight);
        double a = EARTH_RATE * flight;
        return new Vector(
                Math.cos(a) * p.x + Math.sin(a) * p.y, -Math.sin(a) * p.x + Math.cos(a) * p.y, p.z);
    }
    static double range(Ephemeris e, double tow, Vector receiver) {
        double flight = .075;
        for (int i = 0; i < 5; i++)
            flight = received(e, tow, flight).minus(receiver).norm() / C;
        return flight * C;
    }
    private GpsOrbit() {}
}
