package me.idk.justlocation.bridge;

import android.net.wifi.ScanResult;
import android.net.wifi.WifiInfo;
import android.os.SystemClock;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.List;
import java.util.Map;
import java.util.function.Function;

/**
 * Wi-Fi 服务端的替换：在 `WifiServiceImpl` 自己的进程里改它的返回值。
 *
 * <p><b>为什么必须做在服务端</b>：应用拿到的 Wi-Fi 读数都是 `WifiServiceImpl` 经 Binder
 * 回给它的（`WifiManager.getConnectionInfo()` / `getScanResults()` 最终都落到这里），
 * 而 `WifiServiceImpl` 由 Wi-Fi APEX 的 classloader 加载，`system_server` 自己的
 * classloader 看不见它——按类名加载必然失败（真机实测，见备忘录）。
 *
 * <p><b>怎么拿到那个 classloader</b>：ROM 在 `SystemServer.addDexToClassLoader(...)` 时
 * 会把每个独立 jar 的 `PathClassLoader` 登记进
 * `com.android.internal.os.SystemServerClassLoaderFactory.sLoadedPaths` 这张静态表。
 * 该类在 boot classpath 上，**任何进程都能反射读到它**，按 jar 路径即可取出 Wi-Fi 服务
 * 用的那个 classloader。这条路不依赖启动时序，也不需要"接住服务实例"——2026-09-12 从原版
 * 的实现里确认（`C0089.getSystemServiceManagerClassLoader`），并对着 AOSP android-15.0.0_r1
 * 核实过该字段确实存在。
 *
 * <p><b>挂哪两个方法</b>（真机 `service-wifi.jar` 字节码核对的签名，不要照 AOSP 印象写）：
 * <ul>
 *   <li>`getConnectionInfo(String, String)` → `WifiInfo`</li>
 *   <li>`getScanResults(String, String)` → `com.android.wifi.x.com.android.modules.utils.ParceledListSlice`
 *       （APEX 自己那份 slice，不是 framework 的，必须用服务的 classloader 构造）</li>
 * </ul>
 * 两个方法的第一个参数都是调用方包名，作用范围据此判断；权限检查在方法体内部、替换发生在
 * 返回值上，所以调用方该有的权限与异常语义都照旧。
 *
 * <p>与 `WifiOutput`（对象合成）与 `WifiSettings`（配置解析）分工：本类只管"在哪挂、挂上没挂上"。
 */
final class WifiServiceImplHooks {
    /** 系统为这个 jar 单独建的 classloader，Wi-Fi 服务类就在里面。 */
    private static final String SERVICE_JAR = "/apex/com.android.wifi/javalib/service-wifi.jar";
    private static final String FACTORY = "com.android.internal.os.SystemServerClassLoaderFactory";
    private static final String SERVICE = "com.android.server.wifi.WifiServiceImpl";
    private static final String APEX_SLICE = "com.android.wifi.x.com.android.modules.utils.ParceledListSlice";

    private final Class<?> service;
    private final Method connectionInfo;
    private final Method scanResults;
    private final Constructor<?> slice;
    private final Function<String, WifiOutput> source;
    private volatile boolean scanReady, connectionReady;

    /**
     * 两处回调被走到的次数，随心跳交给守护进程，落在状态的 `wifi_hook_calls` 里。
     *
     * <p>为什么要有它：「hook 就绪」只说明方法挂上了，不说明回调真的被走到。
     * 2026-09-12 真机验收里就是"两项都报就绪、应用侧却读到真实数据"，
     * 而所有基于日志/文件的诊断都栽在同一个坑上——`system_server` 既写不进
     * `/data/adb/justlocation`（`drwx------ root root`），启动期日志又会被环形缓冲冲掉。
     * 计数走状态回包本身，没有写入失败这回事。
     */
    private static final java.util.concurrent.atomic.AtomicInteger CALLS =
            new java.util.concurrent.atomic.AtomicInteger();

    static int calls() {
        return CALLS.get();
    }

    /** 只在回调真的进来时加一；放行与替换都算"走到了"。 */
    private static void record() {
        CALLS.incrementAndGet();
    }

    WifiServiceImplHooks(ClassLoader systemServerLoader, Function<String, WifiOutput> source) throws Exception {
        this.source = source;
        ClassLoader serviceLoader = serviceClassLoader(systemServerLoader);
        if (serviceLoader == null) throw new ClassNotFoundException("Wi-Fi 服务的 classloader 未在 SystemServerClassLoaderFactory 中登记");
        service = Class.forName(SERVICE, false, serviceLoader);
        connectionInfo = service.getDeclaredMethod("getConnectionInfo", String.class, String.class);
        scanResults = service.getDeclaredMethod("getScanResults", String.class, String.class);
        // 构造器拿不到就会抛，这里不需要再判空。
        slice = Class.forName(APEX_SLICE, false, serviceLoader).getConstructor(List.class);
    }

    /**
     * 取出 ROM 为某个独立 jar 建的 classloader。
     *
     * <p>用反射读静态表而不是 `getOrCreateClassLoader`：后者对 APEX jar 会拒绝在运行时新建
     * （`allowClassLoaderCreation` 只放行 testOnly 或正在 profile 的情形），而我们要的正是
     * 已登记的那一个。表里没有就返回 null，由调用方决定怎么办，不在这里悄悄新建。
     */
    private static ClassLoader serviceClassLoader(ClassLoader systemServerLoader) {
        try {
            Class<?> factory = Class.forName(FACTORY, false, systemServerLoader);
            Field loaded = factory.getDeclaredField("sLoadedPaths");
            loaded.setAccessible(true);
            Object value = loaded.get(null);
            if (!(value instanceof Map<?, ?> map)) return null;
            // 登记的键是 jar 路径；个别实现会带 ABI 后缀，所以既按整路径找，也按包含关系找。
            Object exact = map.get(SERVICE_JAR);
            if (exact instanceof ClassLoader found) return found;
            for (Map.Entry<?, ?> entry : map.entrySet()) {
                if (entry.getKey() instanceof String key && key.startsWith(SERVICE_JAR)
                        && entry.getValue() instanceof ClassLoader found) {
                    return found;
                }
            }
            return null;
        } catch (Throwable error) {
            android.util.Log.w("JustLocation", "Cannot read SystemServerClassLoaderFactory", error);
            return null;
        }
    }

    boolean scanReady() { return scanReady; }
    boolean connectionReady() { return connectionReady; }

    /** 装载库时就该能确认这个类存在；确认不了说明 ROM 结构不同，整条通道不接管。 */
    boolean usable() { return service != null && connectionInfo != null && scanResults != null && slice != null; }

    /**
     * 调用方是不是系统身份（uid < 10000）。是的话一律放行，让它读到真实值。
     *
     * <p>这不是我们自己的发明，是原版 `C0004.m33()` 的等价物：系统身份不是"要骗的应用"，
     * 让平台自己的组件读到编造的网络身份，轻则界面显示不对，重则影响它们基于 SSID 做的判断。
     *
     * <p><b>它管不到 SystemUI 与设置</b>：那两个在现代 Android 上是普通应用 uid，
     * 要挡住它们只能靠作用范围，别把这条当成"系统界面不会被改"的保证。
     *
     * <p>只在挂钩回调里调用：那时正处在调用方的 Binder 线程上，`getCallingUid()` 拿到的才是调用方。
     * 若方法是同进程直接调用（不经过 Binder），它返回本进程的 uid（system_server 是 1000），
     * 同样按系统身份放行——这正是我们想要的。
     */
    private static boolean systemCaller() {
        return android.os.Binder.getCallingUid() < android.os.Process.FIRST_APPLICATION_UID;
    }

    void install(TelephonyQueries.Installer installer) throws Exception {
        installer.install(connectionInfo, call -> {
            record();
            String packageName = call.arguments[1] instanceof String name ? name : null;
            WifiOutput output = systemCaller() ? null : current(packageName);
            if (output == null) return call.original();
            // 先落到系统实现上：权限、状态与脱敏都照常算完，只是把里面的身份换成合成的。
            Object original = call.original();
            try {
                WifiInfo replacement = output.connectionInfo();
                return replacement == null ? original : replacement;
            } catch (Exception error) {
                android.util.Log.w("JustLocation", "Cannot build Wi-Fi connection info", error);
                return original;
            }
        });
        connectionReady = true;
        installer.install(scanResults, call -> {
            record();
            String packageName = call.arguments[1] instanceof String name ? name : null;
            WifiOutput output = systemCaller() ? null : current(packageName);
            if (output == null) return call.original();
            Object original = call.original();
            try {
                // 返回类型是 APEX 自己那份 ParceledListSlice，只能用服务的 classloader 构造。
                return slice.newInstance(output.scanResults(SystemClock.elapsedRealtimeNanos()));
            } catch (Exception error) {
                android.util.Log.w("JustLocation", "Cannot build Wi-Fi scan results", error);
                return original;
            }
        });
        scanReady = true;
    }

    private WifiOutput current(String packageName) {
        if (packageName == null) return null;
        // 作用范围只看包名；新鲜度由 WifiOutput 自己带的时间戳决定（与服务端快照同一套规则）。
        return source.apply(packageName);
    }
}
