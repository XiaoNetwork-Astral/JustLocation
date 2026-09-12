package me.idk.justlocation.bridge;

import java.lang.reflect.Method;

/** Replace output after the outer method checks permissions, subscriptions and redaction. */
final class ServiceStateQueries {
    interface Source {
        Object replace(String packageName, int slot, Object authorized) throws Exception;
    }
    private final Method query;
    private final Source source;
    ServiceStateQueries(Class<?> service, Source source) throws Exception {
        this.source = source;
        query = service.getDeclaredMethod("getServiceStateForSlot", int.class, boolean.class,
                boolean.class, String.class, String.class);
    }
    void install(MethodHook.Installer installer) throws Exception {
        installer.install(query, call -> {
            Object authorized = call.original();
            return authorized == null ? null
                                      : source.replace((String) call.arguments[4],
                                                (int) call.arguments[1], authorized);
        });
    }
}
