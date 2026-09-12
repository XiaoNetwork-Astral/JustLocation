package me.idk.justlocation.bridge;

import android.net.wifi.ScanResult;
import android.net.wifi.SupplicantState;
import android.net.wifi.WifiInfo;
import android.net.wifi.WifiSsid;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;

/**
 * Convert Wi-Fi settings into Android connection and scan objects. The first network is the
 * connection; scans use the full list. WifiSettings controls scope and expiry, while hidden object
 * APIs are resolved reflectively.
 */
final class WifiOutput {
    /** Neutral capability string for synthetic scan results. */
    private static final String CAPABILITIES = "[ESS]";
    /** Nominal 5 GHz channel width, not a measured value. */
    private static final int CHANNEL_WIDTH_MHZ = 80;

    /**
     * Use a valid synthetic network ID and score so the connection does not appear disconnected.
     */
    private static final int NETWORK_ID = 1000;
    private static final int SCORE = 60;

    private static final java.lang.reflect.Constructor<WifiInfo> NEW_INFO; // WifiInfo()
    private static final java.lang.reflect.Constructor<ScanResult>
            NEW_SCAN; // ScanResult(WifiSsid,String,String,int,int,long,int,int)
    private static final Method SET_SSID; // WifiInfo.setSSID(WifiSsid)
    private static final Method SET_BSSID; // WifiInfo.setBSSID(String)
    private static final Method SET_MAC; // WifiInfo.setMacAddress(String)
    private static final Method SET_RSSI; // WifiInfo.setRssi(int)
    private static final Method SET_LINK_SPEED; // WifiInfo.setLinkSpeed(int)
    private static final Method SET_FREQUENCY; // WifiInfo.setFrequency(int)
    private static final Method SET_NETWORK_ID; // WifiInfo.setNetworkId(int)
    private static final Method SET_STATE; // WifiInfo.setSupplicantState(SupplicantState)
    private static final java.lang.reflect.Field SCORE_FIELD; // WifiInfo.score
    private static final Method FROM_UTF8; // WifiSsid.fromUtf8Text(CharSequence)
    private static final Method SCAN_SSID; // ScanResult.setWifiSsid(WifiSsid)
    private static final Method SCAN_STANDARD; // ScanResult.setWifiStandard(int)
    private static final Method SCAN_CHANNEL_WIDTH; // ScanResult.setChannelWidth(int)

    static {
        java.lang.reflect.Constructor<WifiInfo> newInfo = null;
        java.lang.reflect.Constructor<ScanResult> newScan = null;
        Method setSsid = null, setBssid = null, setMac = null, setRssi = null, setLinkSpeed = null,
               setFrequency = null;
        Method setNetworkId = null, setState = null;
        java.lang.reflect.Field scoreField = null;
        Method fromUtf8 = null, scanSsid = null, scanStandard = null, scanWidth = null;
        try {
            // Required APIs: disable the channel if any is missing.
            newInfo = WifiInfo.class.getDeclaredConstructor();
            newInfo.setAccessible(true);
            newScan = ScanResult.class.getDeclaredConstructor(WifiSsid.class, String.class,
                    String.class, int.class, int.class, long.class, int.class, int.class);
            newScan.setAccessible(true);
            setSsid = WifiInfo.class.getMethod("setSSID", WifiSsid.class);
            fromUtf8 = WifiSsid.class.getMethod("fromUtf8Text", CharSequence.class);
            scanSsid = ScanResult.class.getMethod("setWifiSsid", WifiSsid.class);
        } catch (Throwable error) {
            android.util.Log.w("JustLocation", "Wi-Fi object APIs unavailable", error);
        }
        // Optional setters vary by ROM; a missing setter only omits its field.
        try {
            setBssid = WifiInfo.class.getMethod("setBSSID", String.class);
        } catch (Throwable ignored) {
        }
        try {
            setMac = WifiInfo.class.getMethod("setMacAddress", String.class);
        } catch (Throwable ignored) {
        }
        try {
            setRssi = WifiInfo.class.getMethod("setRssi", int.class);
        } catch (Throwable ignored) {
        }
        try {
            setLinkSpeed = WifiInfo.class.getMethod("setLinkSpeed", int.class);
        } catch (Throwable ignored) {
        }
        try {
            setFrequency = WifiInfo.class.getMethod("setFrequency", int.class);
        } catch (Throwable ignored) {
        }
        try {
            setNetworkId = WifiInfo.class.getMethod("setNetworkId", int.class);
        } catch (Throwable ignored) {
        }
        try {
            setState = WifiInfo.class.getMethod("setSupplicantState", SupplicantState.class);
        } catch (Throwable ignored) {
        }
        try {
            scoreField = WifiInfo.class.getDeclaredField("score");
            scoreField.setAccessible(true);
        } catch (Throwable ignored) {
        }
        try {
            scanStandard = ScanResult.class.getMethod("setWifiStandard", int.class);
        } catch (Throwable ignored) {
        }
        try {
            scanWidth = ScanResult.class.getMethod("setChannelWidth", int.class);
        } catch (Throwable ignored) {
        }
        NEW_INFO = newInfo;
        NEW_SCAN = newScan;
        SET_SSID = setSsid;
        SET_BSSID = setBssid;
        SET_MAC = setMac;
        SET_RSSI = setRssi;
        SET_LINK_SPEED = setLinkSpeed;
        SET_FREQUENCY = setFrequency;
        SET_NETWORK_ID = setNetworkId;
        SET_STATE = setState;
        SCORE_FIELD = scoreField;
        FROM_UTF8 = fromUtf8;
        SCAN_SSID = scanSsid;
        SCAN_STANDARD = scanStandard;
        SCAN_CHANNEL_WIDTH = scanWidth;
    }

    private final SessionSnapshot scope;
    private final boolean enabled;
    private final List<WifiSettings.Target> targets;

    private WifiOutput(SessionSnapshot scope, boolean enabled, List<WifiSettings.Target> targets) {
        this.scope = scope;
        this.enabled = enabled;
        this.targets = List.copyOf(targets);
    }

    /** Whether all required hidden APIs are available. */
    static boolean usable() {
        return NEW_INFO != null && NEW_SCAN != null && SET_SSID != null && FROM_UTF8 != null
                && SCAN_SSID != null;
    }

    /** Whether an optional setter can populate its field. */
    private static boolean has(Method method) {
        return method != null;
    }

    /** Parse settings first, then require the object APIs needed for complete output. */
    static WifiOutput parse(String response, long nowMs) throws Exception {
        if (!usable())
            return null;
        WifiSettings settings = WifiSettings.parse(WifiSettings.fromJson(response), nowMs);
        if (settings == null)
            return null;
        return new WifiOutput(settings.scope(), true, settings.targets());
    }

    /** Whether simulation applies to this application. */
    boolean appliesTo(String packageName, long nowMs) {
        return enabled && !targets.isEmpty() && scope.appliesTo(packageName, nowMs);
    }

    /** Network name for diagnostics. */
    String connectedSsid() {
        return targets.get(0).ssid();
    }

    /**
     * Build the configured connection state. getApplicableRedactions reports supported redactions,
     * not the caller's required mask, so it must not be passed directly to makeCopy. Populate
     * connection state and network ID as well as identity fields.
     */
    WifiInfo connectionInfo() throws Exception {
        WifiSettings.Target target = targets.get(0);
        WifiInfo info = NEW_INFO.newInstance();
        SET_SSID.invoke(info, ssid(target.ssid()));
        if (!target.bssid().isEmpty()) {
            if (has(SET_BSSID))
                SET_BSSID.invoke(info, target.bssid());
            if (has(SET_MAC))
                SET_MAC.invoke(info, target.bssid());
        }
        if (has(SET_RSSI))
            SET_RSSI.invoke(info, target.rssi());
        if (has(SET_LINK_SPEED))
            SET_LINK_SPEED.invoke(info, target.linkSpeed());
        if (has(SET_FREQUENCY))
            SET_FREQUENCY.invoke(info, target.frequency());
        if (has(SET_NETWORK_ID))
            SET_NETWORK_ID.invoke(info, NETWORK_ID);
        if (has(SET_STATE))
            SET_STATE.invoke(info, SupplicantState.COMPLETED);
        try {
            if (SCORE_FIELD != null)
                SCORE_FIELD.setInt(info, SCORE);
        } catch (Throwable ignored) {
        }
        return info;
    }

    /** Build one scan result for each configured network. */
    List<ScanResult> scanResults(long elapsedMs) throws Exception {
        List<ScanResult> results = new ArrayList<>(targets.size());
        for (WifiSettings.Target target : targets) {
            ScanResult result = NEW_SCAN.newInstance(ssid(target.ssid()), target.bssid(),
                    CAPABILITIES, target.rssi(), target.frequency(), elapsedMs, 0, 0);
            // Missing optional setters retain constructor defaults.
            if (has(SCAN_STANDARD))
                SCAN_STANDARD.invoke(result, ScanResult.WIFI_STANDARD_11AX);
            if (has(SCAN_CHANNEL_WIDTH))
                SCAN_CHANNEL_WIDTH.invoke(result, CHANNEL_WIDTH_MHZ);
            results.add(result);
        }
        return results;
    }

    /**
     * Use fromUtf8Text for SSIDs. fromString treats unquoted input as hexadecimal and can corrupt
     * textual network names.
     */
    private static WifiSsid ssid(String text) throws Exception {
        return (WifiSsid) FROM_UTF8.invoke(null, text);
    }
}
