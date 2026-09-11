package me.idk.justlocation.bridge;

import org.junit.Test;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.function.Function;
import static org.junit.Assert.*;

public class ProviderDispatcherTest {
    public static class Identity {
        final String name;
        Identity(String name) { this.name = name; }
        public String getPackageName() { return name; }
    }
    public static class Registration {
        final Identity identity;
        final boolean active, permitted;
        final List<String> received = new ArrayList<>();
        Registration(String name, boolean active, boolean permitted) {
            identity = new Identity(name); this.active = active; this.permitted = permitted;
        }
        public Identity getIdentity() { return identity; }
        Object acceptLocationChange(String point) {
            return permitted ? (Runnable) () -> received.add(point) : null;
        }
    }
    public static class Multiplexer {
        final List<Registration> registrations = new ArrayList<>();
        protected final void deliverToListeners(Function<Registration, Object> action) {
            for (Registration registration : registrations) {
                if (!registration.active) continue;
                Runnable operation = (Runnable) action.apply(registration);
                if (operation != null) operation.run();
            }
        }
    }
    public static class Provider extends Multiplexer {
        public String getName() { return "gps"; }
    }
    private ProviderDispatcher dispatcher() throws Exception {
        return new ProviderDispatcher(Provider.class, Registration.class, String.class, Identity.class);
    }
    private SessionSnapshot selected() { return new SessionSnapshot(true, false, Set.of("selected"), 1000); }

    @Test public void deliversWithoutProviderEventButKeepsScopeAndPlatformFiltering() throws Exception {
        Provider provider = new Provider();
        Registration selected = new Registration("selected", true, true);
        Registration other = new Registration("other", true, true);
        Registration inactive = new Registration("selected", false, true);
        Registration denied = new Registration("selected", true, false);
        provider.registrations.addAll(List.of(selected, other, inactive, denied));
        ProviderDispatcher dispatcher = dispatcher();
        dispatcher.track(provider);
        dispatcher.dispatch(selected(), () -> 1500, name -> name + ":point");
        assertEquals(List.of("gps:point"), selected.received);
        assertTrue(other.received.isEmpty());
        assertTrue(inactive.received.isEmpty());
        assertTrue(denied.received.isEmpty());
    }

    @Test public void duplicateTrackingDoesNotDuplicateOutputAndRemovedListenersStayRemoved() throws Exception {
        Provider provider = new Provider();
        Registration selected = new Registration("selected", true, true);
        provider.registrations.add(selected);
        ProviderDispatcher dispatcher = dispatcher();
        dispatcher.track(provider); dispatcher.track(provider);
        dispatcher.dispatch(selected(), () -> 1500, name -> "first");
        provider.registrations.clear();
        dispatcher.dispatch(selected(), () -> 2000, name -> "second");
        assertEquals(List.of("first"), selected.received);
    }

    @Test public void stoppedAndExpiredSnapshotsDoNotDeliver() throws Exception {
        Provider provider = new Provider();
        Registration selected = new Registration("selected", true, true);
        provider.registrations.add(selected);
        ProviderDispatcher dispatcher = dispatcher(); dispatcher.track(provider);
        dispatcher.dispatch(new SessionSnapshot(false, true, Set.of(), 1000), () -> 1500, name -> "stopped");
        dispatcher.dispatch(selected(), () -> 4000, name -> "expired");
        assertTrue(selected.received.isEmpty());
    }
}
