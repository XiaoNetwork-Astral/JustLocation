package me.idk.justlocation.bridge;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.function.ToIntFunction;

/** One immutable ID/slot/default mapping for all virtual subscription queries. */
final class VirtualSubscriptions {
    record Entry(int id, int slot, Object info) {}
    record Version(List<Entry> entries, int defaultId, boolean hasReal) {}
    private final Version version;

    VirtualSubscriptions(List<Entry> entries, int defaultId, boolean hasReal) {
        HashSet<Integer> ids = new HashSet<>(), slots = new HashSet<>();
        for (Entry entry : entries)
            if (entry.id < 0 || entry.id == Integer.MAX_VALUE || entry.slot < 0 || entry.slot > 1
                    || entry.info == null || !ids.add(entry.id) || !slots.add(entry.slot))
                throw new IllegalArgumentException("Invalid virtual subscription mapping");
        if (!entries.isEmpty() && !ids.contains(defaultId))
            throw new IllegalArgumentException("Virtual default subscription is missing");
        version = new Version(List.copyOf(entries), defaultId, hasReal);
    }
    Version version() {
        return version;
    }
    boolean isEmpty() {
        return version.entries.isEmpty();
    }
    int size() {
        return version.entries.size();
    }
    Entry byId(int id) {
        for (Entry entry : version.entries)
            if (entry.id == id)
                return entry;
        return null;
    }
    Entry bySlot(int slot) {
        for (Entry entry : version.entries)
            if (entry.slot == slot)
                return entry;
        return null;
    }
    int defaultId(int original) {
        return version.hasReal || isEmpty() ? original : version.defaultId;
    }
    int slot(int id, int original) {
        if (original >= 0)
            return original;
        if (id == Integer.MAX_VALUE && !version.hasReal)
            id = version.defaultId;
        Entry entry = byId(id);
        return entry == null ? original : entry.slot;
    }
    int id(int slot, int original) {
        if (original >= 0)
            return original;
        if (slot == Integer.MAX_VALUE && !version.hasReal)
            return defaultId(original);
        Entry entry = bySlot(slot);
        return entry == null ? original : entry.id;
    }
    int[] appendIds(int[] original) {
        java.util.LinkedHashSet<Integer> ids = new java.util.LinkedHashSet<>();
        for (int id : original)
            ids.add(id);
        for (Entry entry : version.entries)
            ids.add(entry.id);
        return ids.stream().mapToInt(Integer::intValue).toArray();
    }
    List<Object> additions(List<?> original, ToIntFunction<Object> id, ToIntFunction<Object> slot) {
        List<Object> result = new ArrayList<>();
        for (Entry entry : version.entries) {
            boolean occupied = original.stream().anyMatch(
                    real -> id.applyAsInt(real) == entry.id || slot.applyAsInt(real) == entry.slot);
            if (!occupied)
                result.add(entry.info);
        }
        return result;
    }
}
