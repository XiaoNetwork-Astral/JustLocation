package me.idk.justlocation.bridge;

import android.telephony.SubscriptionInfo;

import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Locale;
import java.util.TreeMap;

import org.json.JSONArray;
import org.json.JSONObject;

/** Encode slot, subscription ID and the public operator fields used by the control client. */
final class PhoneSubscriptions {
    static byte[] encode(List<SubscriptionInfo> records) throws Exception {
        if (records == null)
            return null;
        TreeMap<Integer, SubscriptionInfo> slots = new TreeMap<>();
        for (SubscriptionInfo sub : records) {
            if (sub.getSimSlotIndex() >= 0 && sub.getSimSlotIndex() <= 1
                    && sub.getSubscriptionId() >= 0 && sub.getSubscriptionId() != Integer.MAX_VALUE)
                slots.put(sub.getSimSlotIndex(), sub);
        }
        JSONArray result = new JSONArray();
        for (SubscriptionInfo sub : slots.values()) {
            String country =
                    sub.getCountryIso() == null ? "" : sub.getCountryIso().toLowerCase(Locale.ROOT);
            String carrier = sub.getCarrierName() == null ? "" : sub.getCarrierName().toString();
            carrier = carrier.codePoints()
                              .filter(c -> !Character.isISOControl(c))
                              .limit(32)
                              .collect(StringBuilder::new, StringBuilder::appendCodePoint,
                                      StringBuilder::append)
                              .toString();
            result.put(new JSONObject()
                            .put("id", sub.getSubscriptionId())
                            .put("slot", sub.getSimSlotIndex())
                            .put("mcc", digits(sub.getMccString(), "[0-9]{3}"))
                            .put("mnc", digits(sub.getMncString(), "[0-9]{2,3}"))
                            .put("country", country.matches("[a-z]{2}") ? country : "")
                            .put("carrier", carrier));
        }
        return result.toString().getBytes(StandardCharsets.UTF_8);
    }
    private static String digits(String value, String pattern) {
        return value != null && value.matches(pattern) ? value : "";
    }
}
