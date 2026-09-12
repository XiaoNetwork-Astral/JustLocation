package me.idk.justlocation.bridge;

/** Four pipe-separated public operator fields, in the companion protocol's fixed order. */
final class PhoneOperators {
    private PhoneOperators() {}

    static String encode(String networkName, String simName, String networkCode, String simCode) {
        return String.join(
                "|", field(networkName), field(simName), field(networkCode), field(simCode));
    }

    private static String field(String value) {
        // An unavailable or ambiguous field remains empty without shifting the following fields.
        if (value == null || value.indexOf('|') >= 0)
            return "";
        for (int i = 0; i < value.length(); i++) {
            if (Character.isISOControl(value.charAt(i)))
                return "";
        }
        return value;
    }
}
