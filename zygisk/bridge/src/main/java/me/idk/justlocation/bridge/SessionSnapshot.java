package me.idk.justlocation.bridge;

import java.util.Set;

/** Immutable scope snapshot; an expired heartbeat restores system output. */
public final class SessionSnapshot {
    /**
     * The 20-second expiry window tolerates delayed socket round trips. Explicit stop responses
     * take effect on the next successful heartbeat without waiting for expiry.
     */
    private static final long MAX_AGE_MS = 20_000;
    private final boolean active;
    private final boolean all;
    private final Set<String> packages;
    private final long receivedAtMs;

    public SessionSnapshot(boolean active, boolean all, Set<String> packages, long receivedAtMs) {
        this.active = active;
        this.all = all;
        this.packages = Set.copyOf(packages);
        this.receivedAtMs = receivedAtMs;
    }

    public boolean appliesTo(String packageName, long nowMs) {
        long age = nowMs - receivedAtMs;
        return active && packageName != null && !packageName.isBlank() && age >= 0
                && age < MAX_AGE_MS && (all || packages.contains(packageName));
    }
}
