package me.idk.justlocation.bridge;

import org.junit.Test;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import static org.junit.Assert.*;

public class TelephonyRegistryAdapterTest {
    public static class Events {
        public static final int EVENT_CELL_INFO_CHANGED=1, EVENT_CELL_LOCATION_CHANGED=2,
                EVENT_SIGNAL_STRENGTHS_CHANGED=3, EVENT_SIGNAL_STRENGTH_CHANGED=4, EVENT_SERVICE_STATE_CHANGED=5;
    }
    public interface Listener {
        Object asBinder();
        void onCellInfoChanged(List<?> cells);
        void onCellLocationChanged(Object identity);
        void onSignalStrengthsChanged(Object signal);
        void onSignalStrengthChanged(int signal);
        void onCallStateChanged(int state);
        void onServiceStateChanged(State state);
    }
    public static class Sink implements Listener {
        final Object binder = new Object(); final List<Object> values = new ArrayList<>();
        public Object asBinder() { return binder; }
        public void onCellInfoChanged(List<?> cells) { values.add(cells); }
        public void onCellLocationChanged(Object identity) { values.add(identity); }
        public void onSignalStrengthsChanged(Object signal) { values.add(signal); }
        public void onSignalStrengthChanged(int signal) { values.add(signal); }
        public void onCallStateChanged(int state) { values.add("call:"+state); }
        public void onServiceStateChanged(State state) { values.add(state); }
    }
    public static class Record {
        Listener callback; String callingPackage; int subId=7,phoneId=0;
        Set<Integer> events=Set.of(1,2,3,4); boolean permitted=true,coarsePermitted=true,activeUser=true;
        boolean matchTelephonyCallbackEvent(int event) { return events.contains(event); }
    }
    public static class Signal { public int getGsmSignalStrength() { return 99; } }
    public static class State {
        final String cell, operator;
        State(String cell,String operator) {this.cell=cell;this.operator=operator;}
        public State createLocationInfoSanitizedCopy(boolean coarse) {return new State(null,coarse?null:operator);}
    }
    public static class Registry {
        final List<Record> mRecords=new ArrayList<>();
        final List<List<?>> mCellInfo=new ArrayList<>(List.of(List.of("real cells")));
        final Object[] mCellIdentity={"real identity"};
        final Signal[] mSignalStrength={new Signal()};
        final State[] mServiceState={new State("real cell","real operator")};
        int getPhoneIdFromSubId(int subId) { return 0; }
        boolean validateEventAndUserLocked(Record record,int event) { return record.activeUser && record.events.contains(event); }
        boolean checkCoarseLocationAccess(Record record,int sdk) { assertTrue(sdk==1 || sdk==29); return sdk==1?record.permitted:record.coarsePermitted; }
        boolean checkFineLocationAccess(Record record,int sdk) { assertEquals(29,sdk); return record.permitted; }
    }
    static class Fixture {
        final Registry registry=new Registry(); boolean active=true;
        final TelephonyRegistryAdapter adapter;
        Fixture() throws Exception {
            adapter=new TelephonyRegistryAdapter(Registry.class,Record.class,Listener.class,Events.class,
                (pkg,sub,slot,name,authorized) -> {
                    if(!active || !pkg.equals("selected")) return null;
                    if(name.equals("onServiceStateChanged")) {
                        State state=(State)authorized;
                        return new State(state.cell==null?null:"synthetic cell",state.operator==null?null:"synthetic operator");
                    }
                    return name.equals("onCellInfoChanged") ? List.of("synthetic:"+sub+":"+slot)
                        : name.equals("onSignalStrengthChanged") ? 12 : "synthetic";
                });
            adapter.track(registry);
        }
        Record add(String pkg,Sink sink) throws Exception {
            Record record=new Record();record.callingPackage=pkg;
            record.callback=(Listener)adapter.wrap(registry,pkg,7,sink);
            registry.mRecords.add(record);return record;
        }
    }
    @Test public void periodicAndRealCallbacksAgreeWithoutChangingSharedState() throws Exception {
        Fixture f=new Fixture(); Sink selected=new Sink(),other=new Sink();
        Record r=f.add("selected",selected);f.add("other",other);
        assertSame(selected.binder,r.callback.asBinder());
        r.callback.onCellInfoChanged(List.of("real event"));
        assertEquals(List.of(List.of("synthetic:7:0")),selected.values);
        selected.values.clear();f.adapter.dispatch();
        assertEquals(4,selected.values.size());assertTrue(other.values.isEmpty());
        assertEquals(List.of("real cells"),f.registry.mCellInfo.get(0));
        assertEquals("real identity",f.registry.mCellIdentity[0]);
        r.callback.onCallStateChanged(2);assertEquals("call:2",selected.values.get(4));
    }
    @Test public void permissionsUserAndEventsAreRecheckedBeforePeriodicDelivery() throws Exception {
        Fixture f=new Fixture();Sink sink=new Sink();Record r=f.add("selected",sink);
        r.events=Set.of(1,2);r.permitted=false;f.adapter.dispatch();assertTrue(sink.values.isEmpty());
        r.permitted=true;r.activeUser=false;f.adapter.dispatch();assertTrue(sink.values.isEmpty());
        r.activeUser=true;f.adapter.dispatch();assertEquals(2,sink.values.size());
        f.registry.mRecords.clear();f.adapter.dispatch();assertEquals(2,sink.values.size());
    }
    @Test public void stopRestoresLatestRealCacheOnceAndDoesNotBypassRevokedPermission() throws Exception {
        Fixture f=new Fixture();Sink sink=new Sink();Record r=f.add("selected",sink);
        r.events=Set.of(1);f.adapter.dispatch();sink.values.clear();f.active=false;
        f.registry.mCellInfo.set(0,List.of("new real cells"));
        r.permitted=false;f.adapter.dispatch();assertTrue(sink.values.isEmpty());
        r.permitted=true;f.adapter.dispatch();f.adapter.dispatch();
        assertEquals(List.of(List.of("new real cells")),sink.values);
        r.callback.onCellInfoChanged(List.of("later real cells"));
        assertEquals(List.of("later real cells"),sink.values.get(1));
    }
    @Test public void subscriptionAndSlotFollowRegistrationWhileDefaultSubscriptionResolvesCurrentSlot() throws Exception {
        Fixture f=new Fixture();Sink sink=new Sink();Record r=f.add("selected",sink);
        r.events=Set.of(1);r.phoneId=1;r.subId=9;
        f.adapter.dispatch();assertEquals(List.of(List.of("synthetic:9:1")),sink.values);
    }
    @Test public void serviceStateUsesRedactedCopyForPeriodicDeliveryAndRestoration() throws Exception {
        Fixture f=new Fixture();Sink sink=new Sink();Record r=f.add("selected",sink);r.events=Set.of(5);
        f.adapter.dispatch(); State full=(State)sink.values.remove(0);
        assertEquals("synthetic cell",full.cell);assertEquals("synthetic operator",full.operator);
        r.permitted=false;f.adapter.dispatch();State coarse=(State)sink.values.remove(0);
        assertNull(coarse.cell);assertEquals("synthetic operator",coarse.operator);
        r.coarsePermitted=false;f.adapter.dispatch();State hidden=(State)sink.values.remove(0);
        assertNull(hidden.cell);assertNull(hidden.operator);
        r.callback.onServiceStateChanged(new State(null,null));
        State event=(State)sink.values.remove(0);assertNull(event.cell);assertNull(event.operator);
        f.active=false;f.adapter.dispatch();f.adapter.dispatch();
        assertEquals(1,sink.values.size());State restored=(State)sink.values.get(0);
        assertNull(restored.cell);assertNull(restored.operator);
        assertEquals("real cell",f.registry.mServiceState[0].cell);
    }
}
