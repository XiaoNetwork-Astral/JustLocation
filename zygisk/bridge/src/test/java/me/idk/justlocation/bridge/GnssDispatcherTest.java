package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.concurrent.Executor;
import java.util.function.Function;

import org.junit.Test;

/**
 * Validate delivery after listener registration, including self-driven raw channels when the
 * platform never dispatches.
 */
public class GnssDispatcherTest {
    /** Stand-in for ListenerExecutor.ListenerOperation. */
    public interface Operation {
        void operate(Object listener) throws Throwable;
    }

    /** Match the IGnssStatusListener callback shape. */
    public interface Listener {
        Object asBinder();
        void onGnssStarted();
        void onGnssStopped();
        void onFirstFix(int ttff);
        void onSvStatusChanged(Object status);
    }

    /** Stand-in for CallerIdentity. */
    public static class Identity {
        final String name;
        final boolean permitted;
        Identity(String name, boolean permitted) {
            this.name = name;
            this.permitted = permitted;
        }
        public String getPackageName() {
            return name;
        }
    }

    /** Verify AppOps checks use OP_FINE_LOCATION. */
    public static class AppOps {
        public boolean noteOpNoThrow(int op, Identity identity) {
            assertEquals(1, op);
            return identity.permitted;
        }
    }

    /** Platform registration fixture: identity, listener, executor and active flag. */
    public static class Registration {
        final Identity identity;
        final Object listener;
        boolean active = true;
        final Executor executor = Runnable::run;
        Registration(Identity identity, Object listener) {
            this.identity = identity;
            this.listener = listener;
        }
        public Identity getIdentity() {
            return identity;
        }
        public Executor getExecutor() {
            return executor;
        }
    }

    public static class Provider {
        private final AppOps mAppOpsHelper = new AppOps();
        final List<Registration> registrations = new ArrayList<>();
        protected void deliverToListeners(Function<Registration, Operation> prepare)
                throws Throwable {
            for (Registration registration : registrations) {
                if (!registration.active)
                    continue;
                Operation operation = prepare.apply(registration);
                if (operation != null)
                    operation.operate(registration.listener);
            }
        }
    }

    static class Sink implements Listener {
        final List<String> events = new ArrayList<>();
        final Object binder = new Object();
        public Object asBinder() {
            return binder;
        }
        public void onGnssStarted() {
            events.add("started");
        }
        public void onGnssStopped() {
            events.add("stopped");
        }
        public void onFirstFix(int ttff) {
            events.add("fix:" + ttff);
        }
        public void onSvStatusChanged(Object status) {
            events.add(String.valueOf(status));
        }
    }

    /** Reuse one active snapshot because delivery checks compare snapshot identity. */
    private static final GnssListener.Output OUTPUT =
            new GnssListener.Output(new SessionSnapshot(true, false, Set.of("selected"), 1000),
                    new GnssFrame(31, 121, 0, 0, 0, 10000), true, true);

    private static GnssListener.Output output() {
        return OUTPUT;
    }

    private static GnssDispatcher dispatcher(boolean selfDriven) throws Exception {
        return new GnssDispatcher(Provider.class, Listener.class, Registration.class,
                Identity.class, Operation.class,
                (delegate, name)
                        -> new GnssListener(Listener.class, delegate, name,
                                GnssDispatcherTest::output, () -> 1500L, frame -> "synthetic")
                                .proxy(),
                selfDriven);
    }

    /** Platform-driven delivery honors active registrations, AppOps and scope. */
    @Test
    public void platformDeliveryUsesActiveRegistrationsAppOpsAndCurrentScope() throws Exception {
        var dispatcher = dispatcher(false);
        Provider provider = new Provider();
        var selected = new Sink();
        var denied = new Sink();
        var other = new Sink();
        var inactive = new Sink();
        for (Object[] entry : List.of(new Object[] {"selected", true, selected},
                     new Object[] {"selected", false, denied}, new Object[] {"other", true, other},
                     new Object[] {"selected", true, inactive})) {
            Identity identity = new Identity((String) entry[0], (boolean) entry[1]);
            provider.registrations.add(
                    new Registration(identity, dispatcher.wrap(identity, entry[2])));
        }
        provider.registrations.get(3).active = false;
        dispatcher.track(provider);
        dispatcher.track(provider);
        dispatcher.dispatch();
        // Platform-driven delivery excludes registrations the platform marks inactive.
        assertEquals(List.of("started", "fix:0", "synthetic"), selected.events);
        assertTrue(denied.events.isEmpty());
        assertTrue(other.events.isEmpty());
        assertTrue(inactive.events.isEmpty());
        int settled = selected.events.size();
        provider.registrations.clear();
        dispatcher.dispatch();
        assertEquals(settled, selected.events.size());
    }

    /** Self-driven delivery works without platform dispatch. */
    @Test
    public void selfDrivenDeliveryReachesRegistrationsWithoutPlatformTrigger() throws Exception {
        var dispatcher = dispatcher(true);
        Provider provider = new Provider();
        var selected = new Sink();
        var denied = new Sink();
        var other = new Sink();
        for (Object[] entry : List.of(new Object[] {"selected", true, selected},
                     new Object[] {"selected", false, denied},
                     new Object[] {"other", true, other})) {
            Identity identity = new Identity((String) entry[0], (boolean) entry[1]);
            dispatcher.wrap(identity, entry[2]);
        }
        dispatcher.track(provider);
        // An empty platform registry cannot deliver to these listeners.
        assertTrue(provider.registrations.isEmpty());
        dispatcher.dispatch();
        assertEquals(List.of("started", "fix:0", "synthetic"), selected.events);
        assertTrue(denied.events.isEmpty());
        assertTrue(other.events.isEmpty());
        // Verify registration, dispatch, eligibility, attempt, delivery and failure counters for
        // self-driven delivery.
        List<String> counters = dispatcher.counters();
        assertEquals("3", counters.get(0));
        assertEquals("0", counters.get(5));
        assertEquals("3", counters.get(4));
    }
}
