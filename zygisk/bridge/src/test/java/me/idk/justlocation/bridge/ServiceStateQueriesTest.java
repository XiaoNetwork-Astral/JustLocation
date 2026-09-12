package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import java.lang.reflect.Method;

import org.junit.Test;

public class ServiceStateQueriesTest {
    public static class Service {
        boolean denied, missing;
        public String getServiceStateForSlot(
                int slot, boolean fine, boolean coarse, String pkg, String feature) {
            if (denied)
                throw new SecurityException("phone permission");
            return missing ? null : fine ? "redacted" : "real";
        }
    }
    static class Fixture {
        final Service service = new Service();
        MethodHook hook;
        boolean active = true;
        Fixture() throws Exception {
            new ServiceStateQueries(Service.class,
                    (pkg, slot, original)
                            -> active && pkg.equals("selected") && slot == 0
                            ? "synthetic:" + original
                            : original)
                    .install((target, around) -> {
                        hook = new MethodHook(target, around);
                        hook.setBackup(target);
                    });
        }
        Object query(int slot, boolean redacted, String pkg) throws Throwable {
            return hook.callback(new Object[] {service, slot, redacted, false, pkg, null});
        }
    }
    @Test
    public void receivesOnlyAuthorizedResultAndUsesSlotAndPackage() throws Throwable {
        Fixture f = new Fixture();
        assertEquals("synthetic:real", f.query(0, false, "selected"));
        assertEquals("synthetic:redacted", f.query(0, true, "selected"));
        assertEquals("real", f.query(1, false, "selected"));
        assertEquals("real", f.query(0, false, "other"));
        f.active = false;
        assertEquals("real", f.query(0, false, "selected"));
    }
    @Test
    public void nullAndPermissionFailureArePreserved() throws Throwable {
        Fixture f = new Fixture();
        f.service.missing = true;
        assertNull(f.query(0, false, "selected"));
        f.service.denied = true;
        assertThrows(SecurityException.class, () -> f.query(0, false, "selected"));
    }
}
