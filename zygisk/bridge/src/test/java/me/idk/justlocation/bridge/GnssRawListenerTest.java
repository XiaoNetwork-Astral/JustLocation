package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.concurrent.atomic.AtomicReference;

import org.junit.Test;

public class GnssRawListenerTest {
    public interface Listener {
        void onGnssNavigationMessageReceived(Object value);
    }
    public static class Sink implements Listener {
        final List<Object> values = new ArrayList<>();
        public void onGnssNavigationMessageReceived(Object value) {
            values.add(value);
        }
    }
    @Test
    public void timeSlotsAreSharedAndScopeStopExpiryRestoreRealMessages() throws Throwable {
        var value = new AtomicReference<GnssRawListener.Output>();
        long[] clock = {1000};
        Sink a = new Sink(), b = new Sink(), other = new Sink();
        var ha = handler(a, "selected", value, clock);
        var hb = handler(b, "selected", value, clock);
        var ho = handler(other, "other", value, clock);
        set(value, 12000, true);
        ha.tick();
        ha.tick();
        hb.tick();
        ho.tick();
        assertEquals(1, a.values.size());
        assertEquals(a.values, b.values);
        assertTrue(other.values.isEmpty());
        ((Listener) ha.proxy()).onGnssNavigationMessageReceived("hidden");
        ((Listener) ho.proxy()).onGnssNavigationMessageReceived("real");
        assertEquals(1, a.values.size());
        assertEquals(List.of("real"), other.values);
        set(value, 17000, true);
        ha.tick();
        assertEquals(1, a.values.size());
        set(value, 18000, true);
        ha.tick();
        hb.tick();
        assertEquals(2, a.values.size());
        assertEquals(a.values, b.values);
        set(value, 19000, false);
        ha.tick();
        ((Listener) ha.proxy()).onGnssNavigationMessageReceived("disabled");
        assertEquals("disabled", a.values.get(2));
        set(value, 19000, true);
        clock[0] = 30000;
        ((Listener) ha.proxy()).onGnssNavigationMessageReceived("expired");
        ha.tick();
        assertEquals("expired", a.values.get(3));
        value.set(null);
        ((Listener) ha.proxy()).onGnssNavigationMessageReceived("stopped");
        assertEquals("stopped", a.values.get(4));
    }
    private static GnssRawListener handler(
            Sink sink, String name, AtomicReference<GnssRawListener.Output> value, long[] clock) {
        return new GnssRawListener(Listener.class, sink, name, value::get,
                ()
                        -> clock[0],
                "onGnssNavigationMessageReceived", frame -> List.of(frame.gps.navigationSlot()));
    }
    private static void set(
            AtomicReference<GnssRawListener.Output> value, long millis, boolean enabled) {
        value.set(new GnssRawListener.Output(
                new SessionSnapshot(true, false, Set.of("selected"), 1000),
                new GnssFrame(30, 120, 0, 0, 0,
                        GpsOrbit.UNIX_EPOCH_MS - GpsOrbit.LEAP_SECONDS * 1000L + millis),
                enabled));
    }
}
