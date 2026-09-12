package me.idk.justlocation.bridge;

import org.junit.Test;
import java.lang.reflect.Method;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import static org.junit.Assert.*;

public class TelephonyQueriesTest {
    public interface Callback { void onCellInfo(List<?> cells); }
    public static class Phone { public int getSubId() { return 7; } }
    public static class WorkSource {}
    public static class Service {
        // Optimized system APKs remove these constants; request values remain in the bytecode.
        final Map<String, MethodHook> hooks = new HashMap<>();
        boolean permitted = true, hardDenied;
        int radioQueries;
        Object invoke(String name, Object... arguments) throws Throwable {
            Method method = method(name);
            MethodHook hook = hooks.get(name);
            if (hook == null) {
                try { return method.invoke(this, arguments); }
                catch (java.lang.reflect.InvocationTargetException error) { throw error.getCause(); }
            }
            Object[] values = new Object[arguments.length + 1]; values[0] = this;
            System.arraycopy(arguments, 0, values, 1, arguments.length);
            return hook.callback(values);
        }
        boolean allowed() { if (hardDenied) throw new SecurityException("permission"); return permitted; }
        public Object getAllCellInfo(String pkg, String feature) throws Throwable {
            if (!allowed()) return List.of();
            return invoke("getCachedCellInfo");
        }
        public Object getCellLocation(String pkg, String feature) throws Throwable {
            if (!allowed()) return "empty";
            return invoke("sendRequest", 62, null, 7, null, null, -1L);
        }
        private void requestCellInfoUpdateInternal(int subId, Callback callback, String pkg,
                String feature, WorkSource work) throws Throwable {
            if (!allowed()) { callback.onCellInfo(List.of()); return; }
            if (subId != 7) throw new IllegalArgumentException("subscription");
            invoke("sendRequestAsync", 66, callback, new Phone(), work);
        }
        private List<?> getCachedCellInfo() { radioQueries++; return List.of("real"); }
        private Object sendRequest(int command, Object argument, Integer subId, Phone phone,
                WorkSource work, long timeout) { radioQueries++; return "real identity"; }
        private void sendRequestAsync(int command, Object argument, Phone phone, WorkSource work) {
            radioQueries++; ((Callback) argument).onCellInfo(List.of("real"));
        }
    }
    static Method method(String name) {
        for (Method method : Service.class.getDeclaredMethods()) if (method.getName().equals(name)) {
            method.setAccessible(true); return method;
        }
        throw new AssertionError(name);
    }
    static class Fixture {
        final Service service = new Service();
        boolean active = true;
        final TelephonyQueries.Output output = new TelephonyQueries.Output() {
            public List<?> cells(int subId) { return List.of("synthetic:" + subId); }
            public Object identity(int subId) { return "identity:" + subId; }
        };
        Fixture() throws Exception {
            new TelephonyQueries(Service.class, Phone.class, WorkSource.class, Callback.class,
                    pkg -> active && "selected".equals(pkg) ? output : null).install((target, around) -> {
                MethodHook hook = new MethodHook(target, around); hook.setBackup(target);
                service.hooks.put(target.getName(), hook);
            });
        }
    }
    @Test public void selectedQueriesKeepPermissionChecksAndLeaveOtherCallersAlone() throws Throwable {
        Fixture f = new Fixture();
        assertEquals(List.of("synthetic:" + Integer.MAX_VALUE), f.service.invoke("getAllCellInfo", "selected", null));
        assertEquals("identity:7", f.service.invoke("getCellLocation", "selected", null));
        assertEquals(0, f.service.radioQueries);
        assertEquals(List.of("real"), f.service.invoke("getAllCellInfo", "other", null));
        f.service.permitted = false;
        assertEquals(List.of(), f.service.invoke("getAllCellInfo", "selected", null));
        assertEquals("empty", f.service.invoke("getCellLocation", "selected", null));
        f.service.hardDenied = true;
        assertThrows(SecurityException.class, () -> f.service.invoke("getAllCellInfo", "selected", null));
        f.service.hardDenied = false; f.service.permitted = true;
        assertEquals(List.of("real"), f.service.invoke("getCachedCellInfo"));
    }
    @Test public void asyncResultIsDeliveredOnceOnlyAfterPermissionAndSubscriptionChecks() throws Throwable {
        Fixture f = new Fixture(); var results = new java.util.ArrayList<List<?>>();
        Callback callback = results::add;
        f.service.invoke("requestCellInfoUpdateInternal", 7, callback, "selected", null, null);
        assertEquals(List.of(List.of("synthetic:7")), results); assertEquals(0, f.service.radioQueries);
        assertThrows(IllegalArgumentException.class, () -> f.service.invoke("requestCellInfoUpdateInternal", 8, callback, "selected", null, null));
        f.service.permitted = false;
        f.service.invoke("requestCellInfoUpdateInternal", 7, callback, "selected", null, null);
        assertEquals(List.of(List.of("synthetic:7"), List.of()), results);
    }
    @Test public void stoppedOrExpiredSnapshotRestoresQueriesAndDoesNotLeakCallerContext() throws Throwable {
        Fixture f = new Fixture();
        f.service.invoke("getAllCellInfo", "selected", null);
        f.active = false;
        assertEquals(List.of("real"), f.service.invoke("getAllCellInfo", "selected", null));
        var results = new java.util.ArrayList<List<?>>();
        f.service.invoke("requestCellInfoUpdateInternal", 7, (Callback) results::add, "selected", null, null);
        assertEquals(List.of(List.of("real")), results);
        f.active = true;
        assertEquals("real identity", f.service.invoke("sendRequest", 62, null, 7, null, null, -1L));
    }
    @Test public void partialInstallationStaysInactive() throws Throwable {
        Fixture f = new Fixture(); f.service.hooks.clear();
        assertThrows(IllegalStateException.class, () ->
            new TelephonyQueries(Service.class, Phone.class, WorkSource.class, Callback.class, pkg -> f.output)
                .install((target, around) -> {
                    if (target.getName().equals("getCellLocation")) throw new IllegalStateException("hook failed");
                    MethodHook hook = new MethodHook(target, around); hook.setBackup(target);
                    f.service.hooks.put(target.getName(), hook);
                }));
        assertEquals(List.of("real"), f.service.invoke("getAllCellInfo", "selected", null));
    }
}
