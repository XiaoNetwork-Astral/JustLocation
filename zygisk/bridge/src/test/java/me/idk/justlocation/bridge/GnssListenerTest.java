package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.concurrent.atomic.AtomicReference;

import org.junit.Test;

public class GnssListenerTest {
    public interface Status {
        Object asBinder();
        void onGnssStarted();
        void onGnssStopped();
        void onFirstFix(int ttff);
        void onSvStatusChanged(String value);
    }
    public interface Nmea {
        Object asBinder();
        void onNmeaReceived(long timestamp, String sentence);
    }
    static class Sink implements Status, Nmea {
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
        public void onSvStatusChanged(String value) {
            events.add(value);
        }
        public void onNmeaReceived(long timestamp, String sentence) {
            events.add(timestamp + ":" + sentence);
        }
    }
    final AtomicReference<GnssListener.Output> output = new AtomicReference<>();
    long clock = 1500;
    /** Enable both satellite channels for the default fixture. */
    void enable() {
        enable(true, true);
    }
    void enable(boolean gnssEnabled, boolean nmeaEnabled) {
        output.set(
                new GnssListener.Output(new SessionSnapshot(true, false, Set.of("selected"), 1000),
                        new GnssFrame(31, 121, 5, 1, 20, 12345), gnssEnabled, nmeaEnabled));
    }
    GnssListener handler(Sink sink, String name, Class<?> type) throws Exception {
        return new GnssListener(type, sink, name, output::get, () -> clock, frame -> "synthetic");
    }
    @Test
    public void binderIdentityIsPreservedAndOtherApplicationsReceiveRealEvents() throws Throwable {
        enable();
        Sink sink = new Sink();
        var handler = handler(sink, "other", Status.class);
        Status listener = (Status) handler.proxy();
        assertSame(sink.binder, listener.asBinder());
        listener.onSvStatusChanged("real");
        handler.tick();
        assertEquals(List.of("real"), sink.events);
    }
    @Test
    public void syntheticLifecycleStartsOnceAndStopRestoresUnderlyingNavigation() throws Throwable {
        Sink sink = new Sink();
        var handler = handler(sink, "selected", Status.class);
        Status listener = (Status) handler.proxy();
        listener.onGnssStarted();
        listener.onFirstFix(42);
        sink.events.clear();
        enable();
        handler.tick();
        handler.tick();
        listener.onSvStatusChanged("real");
        listener.onFirstFix(50);
        assertEquals(List.of("started", "fix:0", "synthetic", "synthetic"), sink.events);
        output.set(null);
        handler.tick();
        listener.onSvStatusChanged("restored");
        assertEquals(List.of("started", "fix:0", "synthetic", "synthetic", "stopped", "started",
                             "fix:50", "restored"),
                sink.events);
    }
    @Test
    public void ExpiredStateAndRealStopEndSyntheticLifecycle() throws Throwable {
        enable();
        Sink sink = new Sink();
        var handler = handler(sink, "selected", Status.class);
        Status listener = (Status) handler.proxy();
        handler.tick();
        listener.onGnssStopped();
        assertEquals(3, sink.events.size());
        clock = 25_000;
        handler.tick();
        handler.tick();
        assertEquals(List.of("started", "fix:0", "synthetic", "stopped"), sink.events);
        listener.onSvStatusChanged("real");
        assertEquals("real", sink.events.get(4));
    }
    @Test
    public void nmeaWorksWithoutStatusRegistrationAndResumesRealSentencesOnStop() throws Throwable {
        enable();
        Sink sink = new Sink();
        var handler = handler(sink, "selected", Nmea.class);
        Nmea listener = (Nmea) handler.proxy();
        listener.onNmeaReceived(1, "real");
        assertTrue(sink.events.isEmpty());
        handler.tick();
        assertEquals(6, sink.events.size());
        assertTrue(sink.events.get(0).startsWith("12345:$GPGGA"));
        // Replay queued messages before forwarding new real messages after simulation stops.
        output.set(null);
        listener.onNmeaReceived(2, "real");
        assertEquals("1:real", sink.events.get(6));
        assertEquals("2:real", sink.events.get(7));
        assertEquals(8, sink.events.size());
    }
    @Test
    public void aChannelThatIsSwitchedOffLeavesTheSystemOutputUntouched() throws Throwable {
        // Disabled channels forward system status and messages.
        enable(false, false);
        Sink sink = new Sink();
        var statusHandler = handler(sink, "selected", Status.class);
        var nmeaHandler = handler(sink, "selected", Nmea.class);
        Status status = (Status) statusHandler.proxy();
        Nmea nmea = (Nmea) nmeaHandler.proxy();
        assertFalse(statusHandler.needsTick());
        assertFalse(nmeaHandler.needsTick());
        status.onSvStatusChanged("real");
        nmea.onNmeaReceived(7, "real");
        statusHandler.tick();
        nmeaHandler.tick();
        assertEquals(List.of("real", "7:real"), sink.events);
        assertTrue("Disabled channels must not emit simulated status",
                sink.events.stream().noneMatch(event -> event.equals("synthetic")));
    }
    @Test
    public void turningTheStatusSwitchOffStopsSyntheticStatusButKeepsNmea() throws Throwable {
        // Enabling only NMEA leaves satellite status unchanged.
        enable(false, true);
        Sink sink = new Sink();
        var statusHandler = handler(sink, "selected", Status.class);
        var nmeaHandler = handler(sink, "selected", Nmea.class);
        Status status = (Status) statusHandler.proxy();
        Nmea nmea = (Nmea) nmeaHandler.proxy();
        status.onSvStatusChanged("real");
        statusHandler.tick();
        nmeaHandler.tick();
        assertFalse(sink.events.contains("synthetic"));
        assertTrue(sink.events.stream().anyMatch(event -> event.startsWith("12345:$GPGGA")));
        assertTrue("Real status must still be forwarded", sink.events.contains("real"));
    }
    @Test
    public void sentencesHeldDuringSimulationAreReplayedWhenItStops() throws Throwable {
        // Replay real NMEA messages suppressed during simulation.
        enable();
        Sink sink = new Sink();
        var handler = handler(sink, "selected", Nmea.class);
        Nmea listener = (Nmea) handler.proxy();
        listener.onNmeaReceived(11, "held-a");
        listener.onNmeaReceived(12, "held-b");
        assertTrue("Real messages must be queued during simulation", sink.events.isEmpty());
        handler.tick();
        int synthetic = sink.events.size();
        output.set(null);
        listener.onNmeaReceived(13, "after");
        assertEquals("11:held-a", sink.events.get(synthetic));
        assertEquals("12:held-b", sink.events.get(synthetic + 1));
        assertEquals("13:after", sink.events.get(synthetic + 2));
    }
    @Test
    public void heldSentencesAreBoundedSoAMuteChannelCannotGrowMemory() throws Throwable {
        enable();
        Sink sink = new Sink();
        var handler = handler(sink, "selected", Nmea.class);
        Nmea listener = (Nmea) handler.proxy();
        for (int index = 0; index < 200; index++)
            listener.onNmeaReceived(index, "held-" + index);
        output.set(null);
        listener.onNmeaReceived(999, "after");
        // Retain only the newest messages when the queue reaches its limit.
        assertFalse(sink.events.contains("0:held-0"));
        assertTrue(sink.events.contains("199:held-199"));
        assertTrue(sink.events.contains("999:after"));
    }
}
