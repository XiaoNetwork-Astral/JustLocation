package me.idk.justlocation.bridge;

import org.junit.Test;
import java.lang.reflect.Method;
import java.lang.reflect.InvocationTargetException;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import static org.junit.Assert.*;

public class SubscriptionQueriesTest {
    record Info(int id, String operator, String number) {}
    public static class Service {
        final Map<String, MethodHook> hooks = new HashMap<>();
        boolean allowed = true, hardDenied;
        final Info real = new Info(7, "real", ""); // Platform has already redacted the phone number.
        Object invoke(String name, Object... args) throws Throwable {
            MethodHook hook = hooks.get(name);
            Object[] call = new Object[args.length + 1]; call[0] = this;
            System.arraycopy(args, 0, call, 1, args.length);
            if (hook != null) return hook.callback(call);
            try { return method(name).invoke(this, args); }
            catch (InvocationTargetException e) { throw e.getCause(); }
        }
        void check() { if (hardDenied) throw new SecurityException("phone permission"); }
        public List<Info> getAllSubInfoList(String pkg, String feature) { check(); return allowed ? List.of(real) : List.of(); }
        public List<Info> getActiveSubscriptionInfoList(String pkg, String feature, boolean allProfiles) { return getAllSubInfoList(pkg, feature); }
        public List<Info> getAvailableSubscriptionInfoList(String pkg, String feature) { return getAllSubInfoList(pkg, feature); }
        public List<Info> getAccessibleSubscriptionInfoList(String pkg) { return getAllSubInfoList(pkg, null); }
        public List<Info> getOpportunisticSubscriptions(String pkg, String feature) { return getAllSubInfoList(pkg, feature); }
        public Info getActiveSubscriptionInfo(int id, String pkg, String feature) { check(); return allowed && id == 7 ? real : null; }
        public Info getActiveSubscriptionInfoForSimSlotIndex(int slot, String pkg, String feature) { return getActiveSubscriptionInfo(slot == 0 ? 7 : -1, pkg, feature); }
        public Info getActiveSubscriptionInfoForIccId(String icc, String pkg, String feature) { return getActiveSubscriptionInfo("valid".equals(icc) ? 7 : -1, pkg, feature); }
    }
    static Method method(String name) {
        for (Method method : Service.class.getDeclaredMethods()) if (method.getName().equals(name)) return method;
        throw new AssertionError(name);
    }
    static class Fixture {
        final Service service = new Service(); boolean active = true;
        final SubscriptionQueries.Output output = real -> {
            Info info = (Info) real;
            return new Info(info.id(), "synthetic", info.number());
        };
        void install() throws Exception {
            new SubscriptionQueries(Service.class, pkg -> active && "selected".equals(pkg) ? output : null)
                .install((target, around) -> {
                    MethodHook hook = new MethodHook(target, around); hook.setBackup(target);
                    service.hooks.put(target.getName(), hook);
                });
        }
    }
    @Test public void replacesAuthorizedRecordsWithoutChangingIdentityOrRedactedFields() throws Throwable {
        Fixture f = new Fixture(); f.install();
        Info expected = new Info(7, "synthetic", "");
        for (String name : List.of("getAllSubInfoList", "getAvailableSubscriptionInfoList", "getOpportunisticSubscriptions"))
            assertEquals(List.of(expected), f.service.invoke(name, "selected", null));
        assertEquals(List.of(expected), f.service.invoke("getActiveSubscriptionInfoList", "selected", null, false));
        assertEquals(List.of(expected), f.service.invoke("getAccessibleSubscriptionInfoList", "selected"));
        assertEquals(expected, f.service.invoke("getActiveSubscriptionInfo", 7, "selected", null));
        assertEquals(expected, f.service.invoke("getActiveSubscriptionInfoForSimSlotIndex", 0, "selected", null));
        assertEquals(expected, f.service.invoke("getActiveSubscriptionInfoForIccId", "valid", "selected", null));
        assertEquals("real", f.service.real.operator());
    }
    @Test public void softDenialMissingSubscriptionAndHardDenialRemainUnchanged() throws Throwable {
        Fixture f = new Fixture(); f.install(); f.service.allowed = false;
        assertEquals(List.of(), f.service.invoke("getActiveSubscriptionInfoList", "selected", null, false));
        assertNull(f.service.invoke("getActiveSubscriptionInfo", 7, "selected", null));
        f.service.allowed = true;
        assertNull(f.service.invoke("getActiveSubscriptionInfo", 8, "selected", null));
        f.service.hardDenied = true;
        assertThrows(SecurityException.class, () -> f.service.invoke("getAllSubInfoList", "selected", null));
    }
    @Test public void nonTargetAndStoppedSessionReturnOriginalObjects() throws Throwable {
        Fixture f = new Fixture(); f.install();
        assertSame(f.service.real, f.service.invoke("getActiveSubscriptionInfo", 7, "other", null));
        f.active = false;
        assertSame(f.service.real, f.service.invoke("getActiveSubscriptionInfo", 7, "selected", null));
    }
    @Test public void partialInstallationDoesNotActivateEarlierHooks() throws Throwable {
        Fixture f = new Fixture();
        assertThrows(IllegalStateException.class, () -> new SubscriptionQueries(Service.class, pkg -> f.output)
            .install((target, around) -> {
                if (target.getName().equals("getActiveSubscriptionInfoList")) throw new IllegalStateException("hook failed");
                MethodHook hook = new MethodHook(target, around); hook.setBackup(target);
                f.service.hooks.put(target.getName(), hook);
            }));
        assertEquals(List.of(f.service.real), f.service.invoke("getAllSubInfoList", "selected", null));
    }
}
