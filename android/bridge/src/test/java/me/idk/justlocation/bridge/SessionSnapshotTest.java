package me.idk.justlocation.bridge;

import org.junit.Test;
import java.util.Set;
import static org.junit.Assert.*;

public class SessionSnapshotTest {
    @Test public void selectedPackageOnlyReceivesFreshActiveSnapshot() {
        SessionSnapshot snapshot = new SessionSnapshot(true, false, Set.of("example.selected"), 1000);
        assertTrue(snapshot.appliesTo("example.selected", 2000));
        assertFalse(snapshot.appliesTo("example.other", 2000));
        // 窗口是 `SessionSnapshot.MAX_AGE_MS`（20 秒，2026-09-12 实测后从 3 秒放宽的）。
        // 这里按数字写死是为了"改了常量就得改测试"——放宽窗口是失效保护的一部分，不该被顺手改掉。
        assertTrue(snapshot.appliesTo("example.selected", 19_000));
        assertFalse(snapshot.appliesTo("example.selected", 21_000));
    }

    @Test public void stoppedAndEmptyScopesNeverProduceOutput() {
        assertFalse(new SessionSnapshot(false, true, Set.of(), 1000).appliesTo("example.app", 1000));
        assertFalse(new SessionSnapshot(true, false, Set.of(), 1000).appliesTo("example.app", 1000));
        assertFalse(new SessionSnapshot(true, true, Set.of(), 1000).appliesTo(null, 1000));
    }

    @Test public void globalScopeStillRequiresAValidMonotonicTimestamp() {
        SessionSnapshot snapshot = new SessionSnapshot(true, true, Set.of(), 1000);
        assertTrue(snapshot.appliesTo("example.app", 1000));
        assertFalse(snapshot.appliesTo("example.app", 999));
    }

    @Test public void callerCannotMutatePublishedScope() {
        var packages = new java.util.HashSet<>(Set.of("example.selected"));
        SessionSnapshot snapshot = new SessionSnapshot(true, false, packages, 1000);
        packages.add("example.other");
        assertFalse(snapshot.appliesTo("example.other", 1000));
    }
}
