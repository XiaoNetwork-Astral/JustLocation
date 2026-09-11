package me.idk.justlocation.bridge;

import java.lang.reflect.Field;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.List;
import java.util.function.Function;

/** Android 15 PhoneInterfaceManager: substitute only downstream of its permission checks. */
final class TelephonyQueries {
    interface Installer { void install(Method target, MethodHook.Around around) throws Exception; }
    interface Output {
        List<?> cells(int subscriptionId) throws Exception;
        Object identity(int subscriptionId) throws Exception;
    }
    private final Method cached, request, async, all, location, update, subId, deliver;
    private final int allCommand, locationCommand, updateCommand;
    private final Function<String, Output> source;
    private final ThreadLocal<String> caller = new ThreadLocal<>();
    private volatile boolean ready;

    TelephonyQueries(Class<?> service, Class<?> phone, Class<?> workSource, Class<?> callback,
            Function<String, Output> source) throws Exception {
        this.source = source;
        // Resolve every required signature before installing the first hook.
        cached = service.getDeclaredMethod("getCachedCellInfo");
        request = service.getDeclaredMethod("sendRequest", int.class, Object.class, Integer.class,
                phone, workSource, long.class);
        async = service.getDeclaredMethod("sendRequestAsync", int.class, Object.class, phone, workSource);
        all = service.getDeclaredMethod("getAllCellInfo", String.class, String.class);
        location = service.getDeclaredMethod("getCellLocation", String.class, String.class);
        update = service.getDeclaredMethod("requestCellInfoUpdateInternal", int.class, callback,
                String.class, String.class, workSource);
        subId = phone.getMethod("getSubId");
        deliver = callback.getMethod("onCellInfo", List.class);
        allCommand = constant(service, "CMD_GET_ALL_CELL_INFO", 60);
        locationCommand = constant(service, "CMD_GET_CELL_LOCATION", 62);
        updateCommand = constant(service, "CMD_REQUEST_CELL_INFO_UPDATE", 66);
    }

    void install(Installer installer) throws Exception {
        installer.install(cached, call -> {
            Output output = current();
            return output == null ? call.original() : output.cells(Integer.MAX_VALUE);
        });
        installer.install(request, call -> {
            int command = (int) call.arguments[1];
            if (command != allCommand && command != locationCommand) return call.original();
            Output output = current();
            if (output == null) return call.original();
            int subscription = call.arguments[4] == null ? (int) call.arguments[3]
                    : (int) subId.invoke(call.arguments[4]);
            return command == allCommand ? output.cells(subscription) : output.identity(subscription);
        });
        installer.install(async, call -> {
            if ((int) call.arguments[1] != updateCommand) return call.original();
            Output output = current();
            if (output == null) return call.original();
            List<?> cells = output.cells((int) subId.invoke(call.arguments[3]));
            try { deliver.invoke(call.arguments[2], cells); }
            catch (InvocationTargetException error) { throw error.getCause(); }
            return null;
        });
        installer.install(all, call -> withCaller(call, 1));
        installer.install(location, call -> withCaller(call, 1));
        installer.install(update, call -> withCaller(call, 3));
        ready = true;
    }

    private Output current() {
        String name = caller.get();
        return ready && name != null ? source.apply(name) : null;
    }

    private Object withCaller(MethodHook.Call call, int index) throws Throwable {
        String previous = caller.get();
        try { caller.set((String) call.arguments[index]); return call.original(); }
        finally { if (previous == null) caller.remove(); else caller.set(previous); }
    }

    private static int constant(Class<?> type, String name, int android15Value) throws Exception {
        try {
            Field field = type.getDeclaredField(name); field.setAccessible(true); return field.getInt(null);
        } catch (NoSuchFieldException removedByOptimizer) {
            // Verified in android-15.0.0_r1 and the API 35 AVD TeleService DEX. ROMs need the same check.
            return android15Value;
        }
    }
}
