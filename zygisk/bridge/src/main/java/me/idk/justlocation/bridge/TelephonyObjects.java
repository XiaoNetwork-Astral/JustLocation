package me.idk.justlocation.bridge;

import android.telephony.CellIdentity;
import android.telephony.CellInfo;
import android.telephony.CellSignalStrength;
import android.telephony.SignalStrength;
import android.telephony.SubscriptionInfo;

import java.util.Collection;
import java.util.List;

import org.json.JSONObject;

/** Android 15 object construction; absent cell fields remain UNAVAILABLE. */
final class TelephonyObjects {
    private static final int U = CellInfo.UNAVAILABLE;
    private static final Class<?> I = int.class, L = long.class, S = String.class,
                                  C = Collection.class;
    private TelephonyObjects() {}
    static CellInfo cell(JSONObject cell, long elapsedNanos) throws Exception {
        JSONObject id = cell.getJSONObject("identity");
        String radio = id.getString("radio");
        String mcc = id.optString("mcc", null), mnc = id.optString("mnc", null);
        int dbm = cell.getInt("dbm");
        boolean registered = cell.getBoolean("registered");
        Object identity, signal;
        String suffix;
        switch (radio) {
            case "gsm" -> {
                suffix = "Gsm";
                identity = make("CellIdentityGsm", new Class<?>[] {I, I, I, I, S, S, S, S, C},
                        id.getInt("lac"), id.getInt("cid"), optional(id, "arfcn"),
                        optional(id, "bsic"), mcc, mnc, null, null, List.of());
                signal = make("CellSignalStrengthGsm", new Class<?>[] {I, I, I}, dbm, U, U);
            }
            case "wcdma" -> {
                suffix = "Wcdma";
                identity = make("CellIdentityWcdma",
                        new Class<?>[] {
                                I, I, I, I, S, S, S, S, C, type("ClosedSubscriberGroupInfo")},
                        id.getInt("lac"), id.getInt("cid"), optional(id, "psc"),
                        optional(id, "uarfcn"), mcc, mnc, null, null, List.of(), null);
                signal = make("CellSignalStrengthWcdma", new Class<?>[] {I, I, I, I}, U, U, dbm, U);
            }
            case "lte" -> {
                suffix = "Lte";
                identity = make("CellIdentityLte",
                        new Class<?>[] {I, I, I, I, int[].class, I, S, S, S, S, C,
                                type("ClosedSubscriberGroupInfo")},
                        id.getInt("ci"), optional(id, "pci"), id.getInt("tac"),
                        optional(id, "earfcn"), new int[0], U, mcc, mnc, null, null, List.of(),
                        null);
                signal = make("CellSignalStrengthLte", new Class<?>[] {I, I, I, I, I, I, I}, U, dbm,
                        U, U, U, U, U);
            }
            case "nr" -> {
                suffix = "Nr";
                identity = make("CellIdentityNr",
                        new Class<?>[] {I, I, I, int[].class, S, S, L, S, S, C},
                        optional(id, "pci"), id.getInt("tac"), optional(id, "nrarfcn"), new int[0],
                        mcc, mnc, id.getLong("nci"), null, null, List.of());
                signal = make("CellSignalStrengthNr", new Class<?>[] {I, I, I, I, I, I}, U, U, U,
                        dbm, U, U);
            }
            case "cdma" -> {
                suffix = "Cdma";
                JSONObject position = cell.getJSONObject("position");
                identity = make("CellIdentityCdma", new Class<?>[] {I, I, I, I, I, S, S},
                        id.getInt("nid"), id.getInt("sid"), id.getInt("bid"),
                        (int) Math.round(position.getDouble("longitude") * 14400),
                        (int) Math.round(position.getDouble("latitude") * 14400), null, null);
                signal = make(
                        "CellSignalStrengthCdma", new Class<?>[] {I, I, I, I, I}, dbm, U, U, U, U);
            }
            default -> throw new IllegalArgumentException("Unsupported radio: " + radio);
        }
        CellInfo result;
        if (suffix.equals("Nr")) {
            result = (CellInfo) make("CellInfoNr",
                    new Class<?>[] {I, boolean.class, L, type("CellIdentityNr"),
                            type("CellSignalStrengthNr")},
                    registered ? 1 : 0, registered, elapsedNanos, identity, signal);
        } else {
            result = (CellInfo) make("CellInfo" + suffix, new Class<?>[0]);
            result.getClass()
                    .getMethod("setCellIdentity", type("CellIdentity" + suffix))
                    .invoke(result, identity);
            result.getClass()
                    .getMethod("setCellSignalStrength", type("CellSignalStrength" + suffix))
                    .invoke(result, signal);
            CellInfo.class.getMethod("setRegistered", boolean.class).invoke(result, registered);
            CellInfo.class.getMethod("setTimeStamp", L).invoke(result, elapsedNanos);
            CellInfo.class.getMethod("setCellConnectionStatus", I)
                    .invoke(result, registered ? 1 : 0);
        }
        return result;
    }
    static SubscriptionInfo subscription(JSONObject sub) throws Exception {
        Class<?> builder = type("SubscriptionInfo$Builder");
        Object value = builder.getConstructor().newInstance();
        builder.getMethod("setId", I).invoke(value, sub.getInt("id"));
        builder.getMethod("setSimSlotIndex", I).invoke(value, sub.getInt("slot"));
        builder.getMethod("setMcc", S).invoke(value, sub.getString("mcc"));
        builder.getMethod("setMnc", S).invoke(value, sub.getString("mnc"));
        builder.getMethod("setCountryIso", S).invoke(value, sub.getString("country"));
        builder.getMethod("setCarrierName", CharSequence.class)
                .invoke(value, sub.getString("carrier"));
        builder.getMethod("setDisplayName", CharSequence.class)
                .invoke(value, sub.getString("carrier"));
        return (SubscriptionInfo) builder.getMethod("build").invoke(value);
    }
    static SignalStrength signal(CellSignalStrength serving) throws Exception {
        String[] radios = {"Cdma", "Gsm", "Wcdma", "Tdscdma", "Lte", "Nr"};
        Class<?>[] types = new Class<?>[radios.length];
        Object[] values = new Object[radios.length];
        for (int i = 0; i < radios.length; i++) {
            types[i] = type("CellSignalStrength" + radios[i]);
            values[i] = types[i].isInstance(serving) ? serving
                                                     : types[i].getConstructor().newInstance();
        }
        return (SignalStrength) make("SignalStrength", types, values);
    }
    static SubscriptionInfo operator(SubscriptionInfo original, SubscriptionInfo replacement)
            throws Exception {
        Class<?> builder = type("SubscriptionInfo$Builder");
        Object value = builder.getConstructor(SubscriptionInfo.class).newInstance(original);
        builder.getMethod("setMcc", S).invoke(value, replacement.getMccString());
        builder.getMethod("setMnc", S).invoke(value, replacement.getMncString());
        builder.getMethod("setCountryIso", S).invoke(value, replacement.getCountryIso());
        builder.getMethod("setCarrierName", CharSequence.class)
                .invoke(value, replacement.getCarrierName());
        builder.getMethod("setDisplayName", CharSequence.class)
                .invoke(value, replacement.getDisplayName());
        return (SubscriptionInfo) builder.getMethod("build").invoke(value);
    }
    static CellIdentity emptyIdentity() throws Exception {
        return (CellIdentity) make("CellIdentityGsm", new Class<?>[0]);
    }
    private static int optional(JSONObject object, String key) throws Exception {
        return object.isNull(key) ? U : object.getInt(key);
    }
    private static Class<?> type(String name) throws ClassNotFoundException {
        return Class.forName("android.telephony." + name);
    }
    private static Object make(String name, Class<?>[] types, Object... values) throws Exception {
        return type(name).getConstructor(types).newInstance(values);
    }
}
