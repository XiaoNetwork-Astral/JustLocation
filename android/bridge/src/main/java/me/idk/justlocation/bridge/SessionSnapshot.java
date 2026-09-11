package me.idk.justlocation.bridge;

import java.util.Set;

/** Immutable scope snapshot. An expired backend heartbeat restores real output. */
public final class SessionSnapshot {
    private static final long MAX_AGE_MS = 3000;
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
        return active && packageName != null && !packageName.isBlank()
                && age >= 0 && age < MAX_AGE_MS && (all || packages.contains(packageName));
    }
}
