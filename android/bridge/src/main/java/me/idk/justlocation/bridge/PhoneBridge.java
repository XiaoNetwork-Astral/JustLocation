package me.idk.justlocation.bridge;

import android.app.Application;
import android.content.Context;
import android.os.SystemClock;
import android.util.Log;
import java.lang.reflect.Method;

/** Loaded only in the system phone process, before Application.attach. */
public final class PhoneBridge {
    private static volatile TelephonySnapshot snapshot;
    private static volatile boolean queriesReady;
    private static volatile boolean subscriptionsReady;
    private static boolean attached;
    private static native String readState(int installed, int wifiCalls, byte[] subscriptions, String extra);
    private static native Method hook(Method target, Object callback, Method method);
    private static native boolean deoptimize(Method method);
    private PhoneBridge() {}

    public static void start(ClassLoader parent) throws Exception {
        install(Application.class.getDeclaredMethod("attach", Context.class), call -> {
            Object result = call.original();
            attach((Context) call.arguments[1]);
            return result;
        });
        // attach() is short enough to be inlined into these framework callers.
        for (Method method : android.app.Instrumentation.class.getDeclaredMethods()) {
            if (method.getName().equals("newApplication")) requireDeoptimized(method);
        }
    }

    private static synchronized void attach(Context context) {
        if (attached) return;
        attached = true;
        ClassLoader loader = context.getClassLoader();
        try {
            Class<?> service = Class.forName("com.android.phone.PhoneInterfaceManager", false, loader);
            for (Method method : service.getDeclaredMethods()) {
                if (java.util.Set.of("getAllCellInfo", "getCellLocation", "requestCellInfoUpdateInternal",
                        "requestCellInfoUpdate", "requestCellInfoUpdateWithWorkSource", "sendRequest")
                        .contains(method.getName())) requireDeoptimized(method);
            }
            new TelephonyQueries(service,
                    Class.forName("com.android.internal.telephony.Phone", false, loader),
                    Class.forName("android.os.WorkSource", false, loader),
                    Class.forName("android.telephony.ICellInfoCallback", false, loader), name -> {
                        TelephonySnapshot current = snapshot;
                        if (current == null || !current.cellsEnabled
                                || !current.appliesTo(name, SystemClock.elapsedRealtime())) return null;
                        return new TelephonyQueries.Output() {
                            public java.util.List<?> cells(int subId) throws Exception {
                                return current.cells(subId, SystemClock.elapsedRealtimeNanos());
                            }
                            public Object identity(int subId) throws Exception {
                                return current.identity(subId, SystemClock.elapsedRealtimeNanos());
                            }
                        };
                    }).install(PhoneBridge::install);
            new ServiceStateQueries(service, (name, slot, original) -> {
                if (android.os.Binder.getCallingUid() == android.os.Process.myUid()) return original;
                TelephonySnapshot current = snapshot;
                if (current == null || !current.cellsEnabled
                        || !current.appliesTo(name, SystemClock.elapsedRealtime())) return original;
                return current.serviceState(current.resolveSubscription(Integer.MAX_VALUE, slot),
                        (android.telephony.ServiceState) original);
            }).install(PhoneBridge::install);
            queriesReady = true;
            Log.i("JustLocation", "Phone cell query hooks installed");
        } catch (Exception error) {
            Log.w("JustLocation", "Phone cell queries unavailable", error);
        }
        try {
            Class<?> service = Class.forName("com.android.internal.telephony.subscription.SubscriptionManagerService", false, loader);
            new SubscriptionQueries(service, name -> {
                // Local phone-service work must continue to see the actual subscription database.
                if (android.os.Binder.getCallingUid() == android.os.Process.myUid()) return null;
                TelephonySnapshot current = snapshot;
                if (current == null || !current.simEnabled
                        || !current.appliesTo(name, SystemClock.elapsedRealtime())) return null;
                return original -> current.subscription((android.telephony.SubscriptionInfo) original);
            }).install(PhoneBridge::install);
            subscriptionsReady = true;
            Log.i("JustLocation", "Subscription operator hooks installed");
        } catch (Exception error) {
            Log.w("JustLocation", "Subscription operator queries unavailable", error);
        }
        Thread thread = new Thread(() -> {
            boolean reportedEmpty = false;
            boolean reportedError = false;
            while (!Thread.currentThread().isInterrupted()) {
                try {
                    byte[] cards = null;
                    try {
                        var manager = context.getSystemService(android.telephony.SubscriptionManager.class);
                        if (manager != null) cards = PhoneSubscriptions.encode(manager.getActiveSubscriptionInfoList());
                    } catch (Exception unavailable) { /* Service is not ready yet; retry on the next heartbeat. */ }
                    // Wi-Fi 挂钩只装在 system_server，这个进程不参与，计数恒为 0；
                    // 占位是为了两个进程共用同一条原生通路（签名一致，少一处分叉）。
                    // 最后一个参数是**真实运营商值**（换行分隔四项）：属性上挂的是我们写的模拟值，
                    // 而运营商服务里的真值只有这个进程读得到。守护进程用它核对/还原，
                    // 这样"还原"就不再只依赖本地备份文件。
                    String response = readState(128 | (queriesReady ? 1 : 0) | (subscriptionsReady ? 2 : 0), 0, cards,
                            realOperators(context));
                    // **只在异常时各记一条**：这一位不走就给不出"手机进程到底有没有把心跳送到"，
                    // 而手机进程写不进 /data/adb/justlocation，logcat 是唯一出口。
                    // 正常情况下一句都不打，免得每秒一行把环形缓冲冲掉。
                    if (response == null) {
                        if (!reportedEmpty) {
                            reportedEmpty = true;
                            Log.w("JustLocation", "Phone heartbeat got no reply from the module daemon;"
                                    + " the cell channels stay unverified until this process is restarted");
                        }
                    } else {
                        reportedEmpty = false;
                    }
                    snapshot = TelephonySnapshot.parse(response, SystemClock.elapsedRealtime());
                } catch (Exception error) {
                    if (!reportedError) {
                        reportedError = true;
                        Log.w("JustLocation", "Phone heartbeat failed", error);
                    }
                    snapshot = null;
                }
                try { Thread.sleep(1000); }
                catch (InterruptedException error) { Thread.currentThread().interrupt(); }
            }
        }, "JustLocation-phone");
        thread.setDaemon(true); thread.start();
    }

    /**
     * 本进程读到的**真实**运营商名与 PLMN，四个值用换行分隔（属性值里不会有换行）。
     *
     * <p>顺序固定：`networkOperatorName`、`simOperatorName`、`networkOperator`、`simOperator`。
     * 缺项留空，原生侧会把空项转成"这一轮没有这个值"。
     *
     * <p>这几个 getter 在普通应用进程里读的是系统属性（那正是我们改写的地方），
     * 但**这里是手机进程**：它走运营商服务，拿到的才是真值。
     */
    private static String realOperators(Context context) {
        try {
            var manager = context.getSystemService(android.telephony.TelephonyManager.class);
            if (manager == null) return null;
            return String.join("\n",
                    safe(() -> manager.getNetworkOperatorName()),
                    safe(() -> manager.getSimOperatorName()),
                    safe(() -> manager.getNetworkOperator()),
                    safe(() -> manager.getSimOperator()));
        } catch (Exception error) {
            return null;
        }
    }

    private static String safe(java.util.function.Supplier<String> value) {
        try {
            String text = value.get();
            // 换行会破坏"四项用换行分隔"的约定，直接丢掉这一项。
            return text == null || text.indexOf('\n') >= 0 ? "" : text;
        } catch (Exception error) {
            return "";
        }
    }

    private static void install(Method target, MethodHook.Around around) throws Exception {
        target.setAccessible(true);
        MethodHook callback = new MethodHook(target, around);
        synchronized (callback) {
            Method backup = hook(target, callback, MethodHook.class.getMethod("callback", Object[].class));
            if (backup == null) throw new IllegalStateException("Cannot hook " + target);
            callback.setBackup(backup);
        }
    }

    private static void requireDeoptimized(Method method) {
        if (!deoptimize(method)) throw new IllegalStateException("Cannot remove inlining from " + method);
    }
}
