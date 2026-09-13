package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import org.junit.Test;

public class WifiPermissionsTest {
    public static class Utility {
        boolean allowed = true, mac, settings;
        String attribution;
        public void enforceCanAccessScanResults(String pkg, String tag, int uid, String message) {
            if (!allowed || uid != 10123 || !"example.app".equals(pkg))
                throw new SecurityException("Permission or package ownership denied");
            attribution = tag;
        }
        public boolean checkLocalMacAddressPermission(int uid) {
            return mac;
        }
        public boolean checkNetworkSettingsPermission(int uid) {
            return settings;
        }
        public boolean checkNetworkSetupWizardPermission(int uid) {
            return false;
        }
    }
    public static class Service {
        private final Utility mWifiPermissionsUtil = new Utility();
    }
    @Test
    public void deniedUnknownAndMismatchedCallersNeverGetSyntheticWifi() throws Exception {
        Service service = new Service();
        WifiPermissions permissions = new WifiPermissions(Service.class);
        assertTrue(permissions.scanAllowed(service, "example.app", "feature", 10123));
        assertEquals("feature", service.mWifiPermissionsUtil.attribution);
        assertFalse(permissions.scanAllowed(service, "example.app", null, 10124));
        assertFalse(permissions.scanAllowed(service, "example.other", null, 10123));
        assertFalse(permissions.scanAllowed(service, null, null, 10123));
        service.mWifiPermissionsUtil.allowed = false;
        assertFalse(permissions.scanAllowed(service, "example.app", null, 10123));
    }
    @Test
    public void locationPermissionDoesNotGrantMacOrNetworkSettings() throws Exception {
        Service service = new Service();
        WifiPermissions permissions = new WifiPermissions(Service.class);
        assertEquals(6, permissions.redactions(service, 10123, 7));
        service.mWifiPermissionsUtil.mac = true;
        assertEquals(4, permissions.redactions(service, 10123, 7));
        service.mWifiPermissionsUtil.settings = true;
        assertEquals(0, permissions.redactions(service, 10123, 7));
        assertEquals(8, permissions.redactions(service, 10123, 15));
    }
}
