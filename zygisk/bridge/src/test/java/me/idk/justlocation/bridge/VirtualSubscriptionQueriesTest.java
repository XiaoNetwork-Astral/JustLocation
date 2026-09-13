package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import java.lang.reflect.Method;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import org.junit.Test;

public class VirtualSubscriptionQueriesTest {
    public static class Service {
        boolean real, denied;
        public int getSlotIndex(int id) {
            return real && id == 7 ? 0 : -1;
        }
        public int getSubId(int slot) {
            return real && slot == 0 ? 7 : -1;
        }
        public int getPhoneId(int id) {
            return 0;
        }
        public int getDefaultSubId() {
            return real ? 7 : -1;
        }
        public int getDefaultDataSubId() {
            return getDefaultSubId();
        }
        public int getDefaultVoiceSubId() {
            return getDefaultSubId();
        }
        public int getDefaultSmsSubId() {
            return real ? -2 : -1;
        }
        public int[] getActiveSubIdList(boolean visibleOnly) {
            if (denied)
                throw new SecurityException();
            return real ? new int[] {7} : new int[0];
        }
        public boolean isActiveSubId(int id, String pkg, String attribution) {
            if (denied)
                throw new SecurityException();
            return real && id == 7;
        }
        public int getSimStateForSlotIndex(int slot) {
            return real && slot == 0 ? 2 : 1;
        }
        public boolean hasIccCardUsingSlotIndex(int slot) {
            return real && slot == 0;
        }
    }
    static class Fixture {
        final Service service = new Service();
        final Map<String, MethodHook> hooks = new HashMap<>();
        boolean selected = true;
        final VirtualSubscriptions virtuals = new VirtualSubscriptions(
                List.of(new VirtualSubscriptions.Entry(1900000001, 1, "virtual")), 1900000001,
                false);
        void install() throws Exception {
            new VirtualSubscriptionQueries(Service.class, Service.class, (pkg, attribution) -> {
                assertFalse(SubscriptionQueries.insideOriginal());
                return selected ? new VirtualSubscriptions(
                                          virtuals.version().entries(), 1900000001, service.real)
                                : null;
            }).install((target, around) -> {
                MethodHook hook = new MethodHook(target, around);
                hook.setBackup(target);
                hooks.put(target.getName(), hook);
            });
        }
        Object query(String name, Object... args) throws Throwable {
            Object[] call = new Object[args.length + 1];
            call[0] = service;
            System.arraycopy(args, 0, call, 1, args.length);
            return hooks.get(name).callback(call);
        }
    }
    @Test
    public void noSimEnumerationDefaultsMappingsAndCardStateAgree() throws Throwable {
        Fixture f = new Fixture();
        f.install();
        for (String name : List.of("getDefaultSubId", "getDefaultDataSubId", "getDefaultVoiceSubId",
                     "getDefaultSmsSubId"))
            assertEquals(1900000001, f.query(name));
        assertEquals(1, f.query("getSlotIndex", 1900000001));
        assertEquals(1, f.query("getSlotIndex", Integer.MAX_VALUE));
        assertEquals(1, f.query("getPhoneId", 1900000001));
        assertEquals(1900000001, f.query("getSubId", 1));
        assertArrayEquals(new int[] {1900000001}, (int[]) f.query("getActiveSubIdList", true));
        assertEquals(true, f.query("isActiveSubId", 1900000001, "selected", null));
        assertEquals(10, f.query("getSimStateForSlotIndex", 1));
        assertEquals(true, f.query("hasIccCardUsingSlotIndex", 1));
        assertEquals(-1, f.query("getSlotIndex", 999));
        assertEquals(-1, f.query("getSubId", 2));
        assertEquals(1, f.query("getSimStateForSlotIndex", 0));
        assertEquals(false, f.query("isActiveSubId", 999, "selected", null));
    }
    @Test
    public void physicalCardChoicesPermissionsAndStoppedStateArePreserved() throws Throwable {
        Fixture f = new Fixture();
        f.install();
        f.service.real = true;
        assertEquals(7, f.query("getDefaultSubId"));
        assertEquals(-2, f.query("getDefaultSmsSubId"));
        assertEquals(7, f.query("getSubId", 0));
        assertEquals(2, f.query("getSimStateForSlotIndex", 0));
        assertArrayEquals(new int[] {7, 1900000001}, (int[]) f.query("getActiveSubIdList", true));
        f.service.denied = true;
        assertThrows(SecurityException.class, () -> f.query("getActiveSubIdList", true));
        assertThrows(SecurityException.class,
                () -> f.query("isActiveSubId", 1900000001, "selected", null));
        f.service.denied = false;
        f.selected = false;
        assertEquals(-1, f.query("getSlotIndex", 1900000001));
        assertEquals(1, f.query("getSimStateForSlotIndex", 1));
        assertArrayEquals(new int[] {7}, (int[]) f.query("getActiveSubIdList", true));
    }
}
