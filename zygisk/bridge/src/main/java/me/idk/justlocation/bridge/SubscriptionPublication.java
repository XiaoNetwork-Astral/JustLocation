package me.idk.justlocation.bridge;

import java.util.Objects;

/** Callbacks follow the phone's acknowledgement, so their subsequent queries see the new model. */
final class SubscriptionPublication {
    private String desired;
    private boolean acknowledged;
    private long pendingSince;

    void observe(String version, String applied, long now) {
        if (!Objects.equals(desired, version))
            pendingSince = now;
        desired = version;
        acknowledged = version != null && version.equals(applied);
    }
    boolean canNotify(boolean virtualActive, long now) {
        // If the phone becomes unavailable, restoration follows the shared 20-second expiry.
        return acknowledged || (!virtualActive && now - pendingSince >= 20_000);
    }
}
