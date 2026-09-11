package me.idk.justlocation.bridge;

import org.junit.Test;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.function.Function;
import static org.junit.Assert.*;

public class GnssDispatcherTest {
    public interface Operation { void operate(Object listener) throws Throwable; }
    public static class Identity {
        final String name; final boolean permitted;
        Identity(String name, boolean permitted) { this.name = name; this.permitted = permitted; }
        public String getPackageName() { return name; }
    }
    public static class AppOps {
        public boolean noteOpNoThrow(int op, Identity identity) { assertEquals(1, op); return identity.permitted; }
    }
    public static class Registration {
        final Identity identity; final Object listener; boolean active = true;
        Registration(Identity identity, Object listener) { this.identity = identity; this.listener = listener; }
        public Identity getIdentity() { return identity; }
    }
    public static class Provider {
        private final AppOps mAppOpsHelper = new AppOps();
        final List<Registration> registrations = new ArrayList<>();
        protected void deliverToListeners(Function<Registration, Operation> prepare) throws Throwable {
            for (Registration registration : registrations) {
                if (!registration.active) continue;
                Operation operation = prepare.apply(registration);
                if (operation != null) operation.operate(registration.listener);
            }
        }
    }
    @Test public void periodicOutputUsesActiveRegistrationsAppOpsAndCurrentScope() throws Exception {
        GnssListener.Output output = new GnssListener.Output(new SessionSnapshot(true, false, Set.of("selected"), 1000),
                new GnssFrame(31, 121, 0, 0, 0, 10000), true, true);
        var dispatcher = new GnssDispatcher(Provider.class, GnssListenerTest.Status.class, Registration.class,
                Identity.class, Operation.class, () -> output, () -> 1500, frame -> "synthetic");
        Provider provider = new Provider();
        var selected = new GnssListenerTest.Sink(); var denied = new GnssListenerTest.Sink();
        var other = new GnssListenerTest.Sink(); var inactive = new GnssListenerTest.Sink();
        for (var entry : List.of(new Object[]{"selected", true, selected}, new Object[]{"selected", false, denied},
                new Object[]{"other", true, other}, new Object[]{"selected", true, inactive})) {
            Identity identity = new Identity((String) entry[0], (boolean) entry[1]);
            provider.registrations.add(new Registration(identity, dispatcher.wrap(identity, entry[2])));
        }
        provider.registrations.get(3).active = false;
        dispatcher.track(provider); dispatcher.track(provider); dispatcher.dispatch();
        assertEquals(List.of("started", "fix:0", "synthetic"), selected.events);
        assertTrue(denied.events.isEmpty()); assertTrue(other.events.isEmpty()); assertTrue(inactive.events.isEmpty());
        provider.registrations.clear(); dispatcher.dispatch();
        assertEquals(3, selected.events.size());
    }
}
