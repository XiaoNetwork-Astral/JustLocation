package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicReference;

import org.junit.Test;

public class SubscriptionRegistryTest {
    public static class Listener {
        int calls;
        public void onSubscriptionsChanged() {
            calls++;
        }
    }
    public static class Record {
        Listener onSubscriptionsChangedListenerCallback = new Listener();
        String callingPackage;
        int callerUid = 10001;
        Record(String pkg) {
            callingPackage = pkg;
        }
    }
    public static class Registry {
        final List<Record> mRecords = new ArrayList<>();
    }
    @Test
    public void existingAndNewListenersObserveTransitionsOnceAndRemovalReleasesState()
            throws Exception {
        Registry registry = new Registry();
        Record selected = new Record("selected"), other = new Record("other"),
               system = new Record("selected");
        system.callerUid = 1000;
        registry.mRecords.addAll(List.of(selected, other, system));
        AtomicReference<Object> version = new AtomicReference<>();
        SubscriptionRegistry adapter = new SubscriptionRegistry(Registry.class, Record.class,
                Listener.class, pkg -> "selected".equals(pkg) ? version.get() : null);
        adapter.update(registry);
        assertEquals(0, selected.onSubscriptionsChangedListenerCallback.calls);
        version.set("one");
        adapter.update(registry);
        adapter.update(registry);
        assertEquals(1, selected.onSubscriptionsChangedListenerCallback.calls);
        assertEquals(0, other.onSubscriptionsChangedListenerCallback.calls);
        assertEquals(0, system.onSubscriptionsChangedListenerCallback.calls);
        Record newListener = new Record("selected");
        registry.mRecords.add(newListener);
        adapter.update(registry);
        assertEquals(1, newListener.onSubscriptionsChangedListenerCallback.calls);
        version.set("two");
        adapter.update(registry);
        assertEquals(2, selected.onSubscriptionsChangedListenerCallback.calls);
        registry.mRecords.remove(newListener);
        version.set(null);
        adapter.update(registry);
        adapter.update(registry);
        assertEquals(3, selected.onSubscriptionsChangedListenerCallback.calls);
        assertEquals(2, newListener.onSubscriptionsChangedListenerCallback.calls);
        registry.mRecords.add(newListener);
        adapter.update(registry);
        assertEquals(2, newListener.onSubscriptionsChangedListenerCallback.calls);
    }
}
