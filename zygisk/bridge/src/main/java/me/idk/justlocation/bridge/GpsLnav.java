package me.idk.justlocation.bridge;

/**
 * IS-GPS-200N LNAV encoder. Bit offsets include each word's six parity bits.
 * Output is ten big-endian 30-bit transmitted words in Android's 32-bit containers.
 */
final class GpsLnav {
    record Message(int subframe, int page, byte[] data) {}
    private static final int[][] CHECKS = {{1, 2, 3, 5, 6, 10, 11, 12, 13, 14, 17, 18, 20, 23},
            {2, 3, 4, 6, 7, 11, 12, 13, 14, 15, 18, 19, 21, 24},
            {1, 3, 4, 5, 7, 8, 12, 13, 14, 15, 16, 19, 20, 22},
            {2, 4, 5, 6, 8, 9, 13, 14, 15, 16, 17, 20, 21, 23},
            {1, 3, 5, 6, 7, 9, 10, 14, 15, 16, 17, 18, 21, 22, 24},
            {3, 5, 6, 8, 9, 10, 11, 13, 15, 19, 22, 23, 24}};
    private static final int[] PAGE4 = {57, 25, 26, 27, 28, 57, 29, 30, 31, 32, 57, 62, 52, 53, 54,
            57, 55, 56, 58, 59, 57, 60, 61, 62, 63};
    static Message message(int prn, long slot) {
        long seconds = slot * 6, week = Math.floorDiv(seconds, GpsOrbit.WEEK);
        int tow = (int) Math.floorMod(seconds, GpsOrbit.WEEK);
        int sf = tow / 6 % 5 + 1, page = tow / 30 % 25 + 1;
        var e = GpsOrbit.ephemeris(prn, seconds);
        GpsLnav b = new GpsLnav();
        b.put(0, 8, 0x8b);
        b.put(30, 17, (tow / 6 + 1) % 100800);
        b.put(49, 3, sf);
        switch (sf) {
            case 1 -> {
                b.put(60, 10, week % 1024);
                b.put(70, 2, 1); // L2 P code indication; no L2 measurements are synthesized.
                b.put(210, 8, e.issue());
                b.put(218, 16, e.toe() / 16);
                // Healthy satellite, URA index 0; zero synthetic clock bias/drift and TGD.
            }
            case 2 -> {
                b.put(60, 8, e.issue());
                b.split(106, e.mean());
                b.split(226, GpsOrbit.SQRT_A);
                b.put(270, 16, e.toe() / 16);
            }
            case 3 -> {
                b.split(76, e.node());
                b.split(136, GpsOrbit.INCLINATION);
                b.put(240, 24, GpsOrbit.NODE_RATE);
                b.put(270, 8, e.issue());
            }
            case 4, 5 -> {
                int id = sf == 5 ? (page == 25 ? 51 : page) : PAGE4[page - 1];
                b.put(60, 2, 1);
                b.put(62, 6, id);
                int toa = (tow / 4096) * 4096;
                if (id >= 1 && id <= 32)
                    b.almanac(GpsOrbit.at(id, week, toa));
                else if (sf == 5) {
                    b.put(68, 8, toa / 4096);
                    b.put(76, 8, week % 256);
                    // All 24 health fields are zero (healthy).
                } else if (id == 56) {
                    // No atmospheric errors in the simulation. UTC offset stays at 18s.
                    b.put(218, 8, toa / 4096);
                    b.put(226, 8, week % 256);
                    b.put(240, 8, GpsOrbit.LEAP_SECONDS);
                    b.put(248, 8, week % 256);
                    b.put(256, 8, 7);
                    b.put(270, 8, GpsOrbit.LEAP_SECONDS);
                } else if (id == 63) {
                    // 32 four-bit configurations: anti-spoof off, Block II capability (001).
                    for (int n = 0; n < 32; n++)
                        b.dataField(68, n * 4, 4, 1);
                    // SV25..32 health bits remain zero.
                } else if (id == 55) {
                    byte[] note = "JUSTLOCATION SIMULATION".getBytes(
                            java.nio.charset.StandardCharsets.US_ASCII);
                    for (int n = 0; n < note.length; n++)
                        b.dataField(68, n * 8, 8, note[n]);
                } else if (id == 52) {
                    b.put(68, 2, 3); // NMCT unavailable (reserved/unavailable ERD data).
                }
                // Other pages carry their specified ID and reserved zero data.
            }
            default -> throw new AssertionError();
        }
        return new Message(sf, sf <= 3 ? -1 : page, b.encode());
    }
    private final int[] words = new int[10];
    private void put(int start, int size, long value) {
        for (int i = 0; i < size; i++) {
            int bit = start + i;
            if (bit % 30 >= 24)
                throw new IllegalArgumentException("Field overlaps parity");
            words[bit / 30] |= (int) ((value >>> (size - 1 - i)) & 1) << (29 - bit % 30);
        }
    }
    private void split(int start, long field) {
        put(start, 8, field >>> 24);
        put(start + 14, 24, field);
    }
    private void dataField(int start, int offset, int size, long field) {
        int data = start / 30 * 24 + start % 30 + offset;
        for (int n = 0; n < size; n++)
            put((data + n) / 24 * 30 + (data + n) % 24, 1, field >>> (size - 1 - n));
    }
    private void almanac(GpsOrbit.Ephemeris e) {
        put(90, 8, e.toe() / 4096);
        put(98, 16,
                Math.round((GpsOrbit.radians(GpsOrbit.INCLINATION) / Math.PI - .3) * (1L << 19)));
        put(120, 16, Math.round(Math.scalb((double) GpsOrbit.NODE_RATE, -5)));
        put(150, 24, Math.round(Math.scalb((double) GpsOrbit.SQRT_A, -8)));
        put(180, 24, Math.round(Math.scalb((double) e.node(), -8)));
        put(240, 24, Math.round(Math.scalb((double) e.mean(), -8)));
    }
    private static int parity(int word, int previous) {
        int result = 0;
        for (int i = 0; i < 6; i++) {
            int value = (previous >> ((i == 0 || i == 2 || i == 5) ? 1 : 0)) & 1;
            for (int bit : CHECKS[i])
                value ^= (word >>> (30 - bit)) & 1;
            result = (result << 1) | value;
        }
        return result;
    }
    private byte[] encode() {
        byte[] bytes = new byte[40];
        int previous = 0; // Word 10 of every preceding subframe ends in D29=D30=0.
        for (int i = 0; i < 10; i++) {
            int data = words[i], p = parity(data, previous);
            if (i == 1 || i == 9) {
                // Solve the two non-information-bearing bits so D29/D30 end at zero.
                for (int nib = 0; nib < 4; nib++) {
                    data = (words[i] & ~0xc0) | (nib << 6);
                    p = parity(data, previous);
                    if ((p & 3) == 0)
                        break;
                }
            }
            int transmitted = (data ^ ((previous & 1) == 0 ? 0 : 0x3fffffc0)) | p;
            for (int j = 0; j < 4; j++)
                bytes[i * 4 + j] = (byte) (transmitted >>> (24 - j * 8));
            previous = p;
        }
        return bytes;
    }
    private GpsLnav() {}
}
