package me.idk.justlocation.bridge;

import java.util.HexFormat;
import java.util.Locale;

/** Host fixture producer for the independent JavaScript test receiver. */
public final class GnssModelProbe {
    public static void main(String[] args) {
        double[][] points = {{31.2304, 121.4737, 42, 0, 0}, {-33.86, 151.2, 75, 15, 80},
                {89.9, -179.9, 800, 4, 270}, {0, 0, -300, 0, 0}, {-89.9, 179.9, 12000, 200, 180},
                {45, 179.999, 100, 30, 30}, {45, -179.999, 100, 30, 30}};
        long base = 2436L * 604800; // GPS week boundary; exercises preceding/following weeks.
        int index = 0;
        for (double[] p : points)
            for (int offset : new int[] {-1, 0, 1, 7199, 7200, 7201}) {
                long gpsMs = (base + offset) * 1000 + 123;
                long utc = gpsMs + GpsOrbit.UNIX_EPOCH_MS - GpsOrbit.LEAP_SECONDS * 1000L;
                GpsEpoch epoch = new GpsEpoch(p[0], p[1], p[2], p[3], p[4], utc);
                System.out.printf(Locale.ROOT, "FIX %d %.9f %.9f %.3f %.3f %.3f %d%n", index, p[0],
                        p[1], p[2], p[3], p[4], gpsMs);
                for (var o : epoch.observations) {
                    int prn = o.orbit().id();
                    long frame = Math.floorDiv(gpsMs, 30000) * 5;
                    for (long slot = frame; slot < frame + 3; slot++) {
                        var m = GpsLnav.message(prn, slot);
                        System.out.printf("NAV %d %d %d %d %s%n", index, prn, m.subframe(),
                                m.page(), HexFormat.of().formatHex(m.data()));
                    }
                    System.out.printf(Locale.ROOT, "MEAS %d %d %d %d %.9f %.6f%n", index, prn,
                            epoch.gpsNanos, o.transmitNanos(), o.rangeRate(), o.elevation());
                }
                index++;
            }
        long cycle = (base + 1500) / 6;
        for (long slot = cycle; slot < cycle + 125; slot++) {
            var m = GpsLnav.message(3, slot);
            System.out.printf(
                    "CYCLE %d %d %s%n", m.subframe(), m.page(), HexFormat.of().formatHex(m.data()));
        }
    }
}
