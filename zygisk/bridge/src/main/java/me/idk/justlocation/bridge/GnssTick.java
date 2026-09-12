package me.idk.justlocation.bridge;

/** Periodic GNSS delivery contract; listeners decide whether the snapshot applies. */
interface GnssTick {
    /** Whether this caller has fresh, enabled simulated output. */
    boolean needsTick();

    /** Deliver the current snapshot, rechecking its validity before each event. */
    void tick() throws Throwable;

    /** Track tick entries and actual listener calls separately. */
    default long ticks() {
        return -1;
    }

    default long deliveries() {
        return -1;
    }

    /** Describe the receiver and event interface, or return null when unavailable. */
    default String description() {
        return null;
    }
}
