package me.idk.justlocation.bridge;

import java.util.Set;

/** Immutable scope snapshot. An expired backend heartbeat restores real output. */
public final class SessionSnapshot {
    /**
     * 快照多久算过期。过期后各通道一律恢复系统原值——这是失效保护：
     * 后台停了、或者桥接拿不到状态了，就不该继续喂编造的数据。
     *
     * <p>**为什么是 20 秒而不是 3 秒**（2026-09-12 实测后放宽）：心跳周期本身就是
     * "一次 socket 往返 + sleep 1 秒"，而原生侧单次 socket 超时是 2 秒，所以一次慢往返
     * 就能把快照年龄顶过 3 秒，窗口一开就漏真实数据出去。实测把守护进程 `kill -STOP`
     * 十秒，gps 与 network 两路都返回了真实坐标；`-CONT` 后立刻恢复。
     *
     * <p>放宽**不影响"停止"的即时性**：用户停止是靠回包里的 `requested_active` 变 false
     * 被各通道解析到的，与这个超时无关，下一次心跳（≤1 秒）就生效。它只影响
     * "守护进程彻底失联"这一种情况——那时应用会多撑十几秒才回到真实值。
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
        return active && packageName != null && !packageName.isBlank()
                && age >= 0 && age < MAX_AGE_MS && (all || packages.contains(packageName));
    }
}
