package me.idk.justlocation.bridge;

import org.junit.Test;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.concurrent.Executor;
import java.util.function.Function;
import static org.junit.Assert.*;

/**
 * 投递机制的回归：**应用注册进来之后，数据该不该到、到什么程度**。
 *
 * <p>第二个用例是这轮的关键：导航电文在真机上"钩子被走到、注册也包住了，应用却收到 0 条"，
 * 所以原始通道改成**自己驱动投递**——不依赖平台是否来叫我们。用例把最坏情况固定下来：
 * 平台从来没调用投递、注册表里也什么都没有，只要应用注册过，我们自己也得把数据送到。
 */
public class GnssDispatcherTest {
    /** 平台那个操作接口的替身（真实类型是 ListenerExecutor$ListenerOperation）。 */
    public interface Operation { void operate(Object listener) throws Throwable; }

    /** 监听接口的替身：形状要跟真实的 `IGnssStatusListener` 一致，`onSvStatusChanged` 必须在。 */
    public interface Listener {
        Object asBinder();
        void onGnssStarted(); void onGnssStopped(); void onFirstFix(int ttff);
        void onSvStatusChanged(Object status);
    }

    /** 平台的 `CallerIdentity` 替身。 */
    public static class Identity {
        final String name; final boolean permitted;
        Identity(String name, boolean permitted) { this.name = name; this.permitted = permitted; }
        public String getPackageName() { return name; }
    }

    /** 平台的 `AppOpsHelper` 替身：断言查的就是 `OP_FINE_LOCATION`。 */
    public static class AppOps {
        public boolean noteOpNoThrow(int op, Identity identity) { assertEquals(1, op); return identity.permitted; }
    }

    /** 平台的注册对象替身：身份、监听器、执行器、活跃位。 */
    public static class Registration {
        final Identity identity; final Object listener; boolean active = true;
        final Executor executor = Runnable::run;
        Registration(Identity identity, Object listener) { this.identity = identity; this.listener = listener; }
        public Identity getIdentity() { return identity; }
        public Executor getExecutor() { return executor; }
    }

    public static class Provider {
        private final AppOps mAppOpsHelper = new AppOps();
        final List<Registration> registrations = new ArrayList<>();
        protected void deliverToListeners(Function<Registration, Operation> prepare) throws Throwable {
            for (Registration registration : registrations) {
                if (!registration.active) continue;
                Operation operation = prepare.apply(registration);
                if (operation != null) operation.operate(registration.listener);
            }
        }
    }

    static class Sink implements Listener {
        final List<String> events = new ArrayList<>();
        final Object binder = new Object();
        public Object asBinder() { return binder; }
        public void onGnssStarted() { events.add("started"); }
        public void onGnssStopped() { events.add("stopped"); }
        public void onFirstFix(int ttff) { events.add("fix:" + ttff); }
        public void onSvStatusChanged(Object status) { events.add(String.valueOf(status)); }
    }

    /** 一份"模拟已生效、作用范围只有 selected"的快照。**只造一次**：`current() == value` 这类
     * 新鲜度重查比的是同一个对象，每次新建会让自己跟自己比不过（第一版测试就栽在这里）。 */
    private static final GnssListener.Output OUTPUT = new GnssListener.Output(
            new SessionSnapshot(true, false, Set.of("selected"), 1000), new GnssFrame(31, 121, 0, 0, 0, 10000), true, true);

    private static GnssListener.Output output() { return OUTPUT; }

    private static GnssDispatcher dispatcher(boolean selfDriven) throws Exception {
        return new GnssDispatcher(Provider.class, Listener.class, Registration.class,
                Identity.class, Operation.class,
                (delegate, name) -> new GnssListener(Listener.class, delegate, name,
                        GnssDispatcherTest::output, () -> 1500L, frame -> "synthetic").proxy(),
                selfDriven);
    }

    /** 平台驱动（卫星状态、NMEA 用）：只有活跃注册收到，AppOps 与作用范围说了算。 */
    @Test public void platformDeliveryUsesActiveRegistrationsAppOpsAndCurrentScope() throws Exception {
        var dispatcher = dispatcher(false);
        Provider provider = new Provider();
        var selected = new Sink(); var denied = new Sink();
        var other = new Sink(); var inactive = new Sink();
        for (Object[] entry : List.of(new Object[]{"selected", true, selected}, new Object[]{"selected", false, denied},
                new Object[]{"other", true, other}, new Object[]{"selected", true, inactive})) {
            Identity identity = new Identity((String) entry[0], (boolean) entry[1]);
            provider.registrations.add(new Registration(identity, dispatcher.wrap(identity, entry[2])));
        }
        provider.registrations.get(3).active = false;
        dispatcher.track(provider); dispatcher.track(provider); dispatcher.dispatch();
        // 平台驱动不会去碰我们自己收集的注册，所以 inactive 那条（平台说它没注册上）收不到。
        assertEquals(List.of("started", "fix:0", "synthetic"), selected.events);
        assertTrue(denied.events.isEmpty()); assertTrue(other.events.isEmpty()); assertTrue(inactive.events.isEmpty());
        int settled = selected.events.size();
        provider.registrations.clear(); dispatcher.dispatch();
        assertEquals(settled, selected.events.size());
    }

    /** 自己驱动（原始测量、导航电文用）：平台一次都不叫，也要送到。 */
    @Test public void selfDrivenDeliveryReachesRegistrationsWithoutPlatformTrigger() throws Exception {
        var dispatcher = dispatcher(true);
        Provider provider = new Provider();
        var selected = new Sink(); var denied = new Sink(); var other = new Sink();
        for (Object[] entry : List.of(new Object[]{"selected", true, selected}, new Object[]{"selected", false, denied},
                new Object[]{"other", true, other})) {
            Identity identity = new Identity((String) entry[0], (boolean) entry[1]);
            dispatcher.wrap(identity, entry[2]);
        }
        dispatcher.track(provider);
        // 平台的注册表是空的：deliverToListeners 一次都不会套到我们头上。
        assertTrue(provider.registrations.isEmpty());
        dispatcher.dispatch();
        assertEquals(List.of("started", "fix:0", "synthetic"), selected.events);
        assertTrue(denied.events.isEmpty()); assertTrue(other.events.isEmpty());
        // 计数串 = 注册数, dispatch 次数, 该投次数, 真投次数, 送达数, 失败数。
        // 这条正是真机上用来分辨"没挂上钩子"和"挂上了但送不到"的观测手段，所以顺手固定下来：
        // 三条注册都被投到、没有失败，就说明这条自己驱动的路真的走通了。
        List<String> counters = dispatcher.counters();
        assertEquals("3", counters.get(0));
        assertEquals("0", counters.get(5));
        assertEquals("3", counters.get(4));
    }
}
