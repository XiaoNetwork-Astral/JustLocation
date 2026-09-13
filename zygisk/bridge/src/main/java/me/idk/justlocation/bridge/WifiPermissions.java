package me.idk.justlocation.bridge;

import java.lang.reflect.Field;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;

/** Reuse the Wi-Fi service's UID/package, profile, location and AppOps checks. */
final class WifiPermissions {
    // Android 15 NetworkCapabilities redaction flags (hidden SDK constants).
    private static final long LOCATION = 1, LOCAL_MAC = 2, NETWORK_SETTINGS = 4;
    private final Field utility;
    private final Method scan, mac, settings, setup;

    WifiPermissions(Class<?> service) throws Exception {
        utility = service.getDeclaredField("mWifiPermissionsUtil");
        utility.setAccessible(true);
        Class<?> type = utility.getType();
        scan = type.getMethod(
                "enforceCanAccessScanResults", String.class, String.class, int.class, String.class);
        mac = type.getMethod("checkLocalMacAddressPermission", int.class);
        settings = type.getMethod("checkNetworkSettingsPermission", int.class);
        setup = type.getMethod("checkNetworkSetupWizardPermission", int.class);
    }

    boolean scanAllowed(Object service, String pkg, String attribution, int uid) throws Exception {
        if (pkg == null || pkg.isBlank() || uid < 0)
            return false;
        try {
            scan.invoke(utility.get(service), pkg, attribution, uid, null);
            return true;
        } catch (InvocationTargetException error) {
            if (error.getCause() instanceof SecurityException)
                return false;
            throw error;
        }
    }

    long redactions(Object service, int uid, long applicable) throws Exception {
        Object checker = utility.get(service);
        long result = applicable & ~LOCATION; // Caller already passed scanAllowed.
        if ((boolean) mac.invoke(checker, uid))
            result &= ~LOCAL_MAC;
        if ((boolean) settings.invoke(checker, uid) || (boolean) setup.invoke(checker, uid))
            result &= ~NETWORK_SETTINGS;
        return result;
    }
}
