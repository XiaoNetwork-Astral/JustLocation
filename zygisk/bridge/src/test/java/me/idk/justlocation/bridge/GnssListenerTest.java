package me.idk.justlocation.bridge;

import org.junit.Test;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.concurrent.atomic.AtomicReference;
import static org.junit.Assert.*;

public class GnssListenerTest {
    public interface Status {
        Object asBinder();
        void onGnssStarted(); void onGnssStopped(); void onFirstFix(int ttff);
        void onSvStatusChanged(String value);
    }
    public interface Nmea { Object asBinder(); void onNmeaReceived(long timestamp, String sentence); }
    static class Sink implements Status, Nmea {
        final List<String> events = new ArrayList<>();
        final Object binder = new Object();
        public Object asBinder() { return binder; }
        public void onGnssStarted() { events.add("started"); }
        public void onGnssStopped() { events.add("stopped"); }
        public void onFirstFix(int ttff) { events.add("fix:" + ttff); }
        public void onSvStatusChanged(String value) { events.add(value); }
        public void onNmeaReceived(long timestamp, String sentence) { events.add(timestamp + ":" + sentence); }
    }
    final AtomicReference<GnssListener.Output> output = new AtomicReference<>();
    long clock = 1500;
    /** 两个卫星通道都打开：这是先前测试一直假设的状态。 */
    void enable() { enable(true, true); }
    void enable(boolean gnssEnabled, boolean nmeaEnabled) {
        output.set(new GnssListener.Output(new SessionSnapshot(true, false, Set.of("selected"), 1000),
                new GnssFrame(31, 121, 5, 1, 20, 12345), gnssEnabled, nmeaEnabled));
    }
    GnssListener handler(Sink sink, String name, Class<?> type) throws Exception {
        return new GnssListener(type, sink, name, output::get, () -> clock, frame -> "synthetic");
    }
    @Test public void binderIdentityIsPreservedAndOtherApplicationsReceiveRealEvents() throws Throwable {
        enable(); Sink sink = new Sink(); var handler = handler(sink, "other", Status.class);
        Status listener = (Status) handler.proxy();
        assertSame(sink.binder, listener.asBinder());
        listener.onSvStatusChanged("real"); handler.tick();
        assertEquals(List.of("real"), sink.events);
    }
    @Test public void syntheticLifecycleStartsOnceAndStopRestoresUnderlyingNavigation() throws Throwable {
        Sink sink = new Sink(); var handler = handler(sink, "selected", Status.class);
        Status listener = (Status) handler.proxy();
        listener.onGnssStarted(); listener.onFirstFix(42); sink.events.clear();
        enable(); handler.tick(); handler.tick();
        listener.onSvStatusChanged("real"); listener.onFirstFix(50);
        assertEquals(List.of("started", "fix:0", "synthetic", "synthetic"), sink.events);
        output.set(null); handler.tick(); listener.onSvStatusChanged("restored");
        assertEquals(List.of("started", "fix:0", "synthetic", "synthetic", "stopped", "started", "fix:50", "restored"), sink.events);
    }
    @Test public void ExpiredStateAndRealStopEndSyntheticLifecycle() throws Throwable {
        enable(); Sink sink = new Sink(); var handler = handler(sink, "selected", Status.class);
        Status listener = (Status) handler.proxy(); handler.tick();
        listener.onGnssStopped(); assertEquals(3, sink.events.size());
        clock = 25_000; handler.tick(); handler.tick();
        assertEquals(List.of("started", "fix:0", "synthetic", "stopped"), sink.events);
        listener.onSvStatusChanged("real"); assertEquals("real", sink.events.get(4));
    }
    @Test public void nmeaWorksWithoutStatusRegistrationAndResumesRealSentencesOnStop() throws Throwable {
        enable(); Sink sink = new Sink(); var handler = handler(sink, "selected", Nmea.class);
        Nmea listener = (Nmea) handler.proxy();
        listener.onNmeaReceived(1, "real"); assertTrue(sink.events.isEmpty());
        handler.tick(); assertEquals(6, sink.events.size());
        assertTrue(sink.events.get(0).startsWith("12345:$GPGGA"));
        // 停止后先补发模拟期间挡下的报文，再继续转发真实报文：应用看到的报文流是连续的。
        output.set(null); listener.onNmeaReceived(2, "real");
        assertEquals("1:real", sink.events.get(6));
        assertEquals("2:real", sink.events.get(7));
        assertEquals(8, sink.events.size());
    }
    @Test public void aChannelThatIsSwitchedOffLeavesTheSystemOutputUntouched() throws Throwable {
        // 两个开关都关：应用应该看到系统原样的状态与报文，而不是合成数据。
        enable(false, false); Sink sink = new Sink(); var statusHandler = handler(sink, "selected", Status.class);
        var nmeaHandler = handler(sink, "selected", Nmea.class);
        Status status = (Status) statusHandler.proxy(); Nmea nmea = (Nmea) nmeaHandler.proxy();
        assertFalse(statusHandler.needsTick());
        assertFalse(nmeaHandler.needsTick());
        status.onSvStatusChanged("real"); nmea.onNmeaReceived(7, "real");
        statusHandler.tick(); nmeaHandler.tick();
        assertEquals(List.of("real", "7:real"), sink.events);
        assertTrue("关掉的通道不该产出合成状态", sink.events.stream().noneMatch(event -> event.equals("synthetic")));
    }
    @Test public void turningTheStatusSwitchOffStopsSyntheticStatusButKeepsNmea() throws Throwable {
        // 只开 NMEA：状态通道原样转发，报文通道仍然按合成投递。
        enable(false, true); Sink sink = new Sink(); var statusHandler = handler(sink, "selected", Status.class);
        var nmeaHandler = handler(sink, "selected", Nmea.class);
        Status status = (Status) statusHandler.proxy(); Nmea nmea = (Nmea) nmeaHandler.proxy();
        status.onSvStatusChanged("real"); statusHandler.tick(); nmeaHandler.tick();
        assertFalse(sink.events.contains("synthetic"));
        assertTrue(sink.events.stream().anyMatch(event -> event.startsWith("12345:$GPGGA")));
        assertTrue("真实状态仍要照常转发", sink.events.contains("real"));
    }
    @Test public void sentencesHeldDuringSimulationAreReplayedWhenItStops() throws Throwable {
        // 模拟期间到达的真实报文不能丢：停止后要按原样补发，报文流才不会中断。
        enable(); Sink sink = new Sink(); var handler = handler(sink, "selected", Nmea.class);
        Nmea listener = (Nmea) handler.proxy();
        listener.onNmeaReceived(11, "held-a"); listener.onNmeaReceived(12, "held-b");
        assertTrue("模拟期间不直接转发真实报文", sink.events.isEmpty());
        handler.tick();
        int synthetic = sink.events.size();
        output.set(null);
        listener.onNmeaReceived(13, "after");
        assertEquals("11:held-a", sink.events.get(synthetic));
        assertEquals("12:held-b", sink.events.get(synthetic + 1));
        assertEquals("13:after", sink.events.get(synthetic + 2));
    }
    @Test public void heldSentencesAreBoundedSoAMuteChannelCannotGrowMemory() throws Throwable {
        enable(); Sink sink = new Sink(); var handler = handler(sink, "selected", Nmea.class);
        Nmea listener = (Nmea) handler.proxy();
        for (int index = 0; index < 200; index++) listener.onNmeaReceived(index, "held-" + index);
        output.set(null); listener.onNmeaReceived(999, "after");
        // 只保留最近的一批，最早的被丢掉。
        assertFalse(sink.events.contains("0:held-0"));
        assertTrue(sink.events.contains("199:held-199"));
        assertTrue(sink.events.contains("999:after"));
    }
}
