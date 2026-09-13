package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import org.junit.Test;

public class SubscriptionPublicationTest {
    @Test
    public void callbacksWaitForAppliedVersionAndLostPhoneRestoresAfterExpiry() {
        SubscriptionPublication state = new SubscriptionPublication();
        state.observe("jl-first", "off", 1000);
        assertFalse(state.canNotify(true, 1001));
        state.observe("jl-first", "jl-first", 2000);
        assertTrue(state.canNotify(true, 2001));
        state.observe("jl-second", "jl-first", 3000);
        assertFalse(state.canNotify(true, 30_000));
        state.observe("off", "jl-first", 4000);
        assertFalse(state.canNotify(false, 4001));
        state.observe("off", null, 5000);
        assertFalse(state.canNotify(false, 23_999));
        assertTrue(state.canNotify(false, 24_000));
        state.observe("off", "off", 6000);
        assertTrue(state.canNotify(false, 6001));
    }
}
