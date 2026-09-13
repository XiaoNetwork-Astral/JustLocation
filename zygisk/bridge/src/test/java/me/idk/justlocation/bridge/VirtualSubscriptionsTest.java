package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import java.util.List;

import org.junit.Test;

public class VirtualSubscriptionsTest {
    private final List<VirtualSubscriptions.Entry> entries =
            List.of(new VirtualSubscriptions.Entry(1900000000, 0, "first"),
                    new VirtualSubscriptions.Entry(1900000001, 1, "second"));
    @Test
    public void defaultsMappingsAndUnknownIdsUseOneModel() {
        VirtualSubscriptions model = new VirtualSubscriptions(entries, 1900000001, false);
        assertEquals(1900000001, model.defaultId(-1));
        assertEquals(1, model.slot(Integer.MAX_VALUE, -1));
        assertEquals(1900000001, model.id(Integer.MAX_VALUE, -1));
        assertEquals(0, model.slot(1900000000, -1));
        assertEquals(1900000001, model.id(1, -1));
        assertNull(model.byId(-1));
        assertNull(model.byId(123));
        assertNull(model.bySlot(2));
        assertEquals(-1, model.slot(123, -1));
        assertEquals(-1, model.id(-1, -1));
        assertArrayEquals(
                new int[] {7, 1900000001, 1900000000}, model.appendIds(new int[] {7, 1900000001}));
    }
    @Test
    public void realDefaultsAndOccupiedSlotsRemainReal() {
        VirtualSubscriptions model = new VirtualSubscriptions(entries, 1900000001, true);
        assertEquals(7, model.defaultId(7));
        assertEquals(-2, model.defaultId(-2));
        assertEquals(7, model.id(0, 7));
        assertEquals(1, model.slot(1900000000, 1));
        record Real(int id, int slot) {}
        assertEquals(List.of("second"),
                model.additions(
                        List.of(new Real(7, 0)), r -> ((Real) r).id(), r -> ((Real) r).slot()));
        assertEquals(List.of(),
                model.additions(List.of(new Real(7, 0), new Real(9, 1)),
                        r -> ((Real) r).id(), r -> ((Real) r).slot()));
    }
}
