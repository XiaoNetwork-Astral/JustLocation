package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import java.util.Set;

import org.junit.Test;

public class SessionSnapshotTest {
    @Test
    public void selectedPackageOnlyReceivesFreshActiveSnapshot() {
        SessionSnapshot snapshot =
                new SessionSnapshot(true, false, Set.of("example.selected"), 1000);
        assertTrue(snapshot.appliesTo("example.selected", 2000));
        assertFalse(snapshot.appliesTo("example.other", 2000));
        // Keep the expected 20-second expiry explicit so changing the policy requires updating
        // this test.
        assertTrue(snapshot.appliesTo("example.selected", 19_000));
        assertFalse(snapshot.appliesTo("example.selected", 21_000));
    }

    @Test
    public void stoppedAndEmptyScopesNeverProduceOutput() {
        assertFalse(
                new SessionSnapshot(false, true, Set.of(), 1000).appliesTo("example.app", 1000));
        assertFalse(
                new SessionSnapshot(true, false, Set.of(), 1000).appliesTo("example.app", 1000));
        assertFalse(new SessionSnapshot(true, true, Set.of(), 1000).appliesTo(null, 1000));
    }

    @Test
    public void globalScopeStillRequiresAValidMonotonicTimestamp() {
        SessionSnapshot snapshot = new SessionSnapshot(true, true, Set.of(), 1000);
        assertTrue(snapshot.appliesTo("example.app", 1000));
        assertFalse(snapshot.appliesTo("example.app", 999));
    }

    @Test
    public void callerCannotMutatePublishedScope() {
        var packages = new java.util.HashSet<>(Set.of("example.selected"));
        SessionSnapshot snapshot = new SessionSnapshot(true, false, packages, 1000);
        packages.add("example.other");
        assertFalse(snapshot.appliesTo("example.other", 1000));
    }
}
