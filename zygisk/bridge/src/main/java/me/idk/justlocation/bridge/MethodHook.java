package me.idk.justlocation.bridge;

import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.Arrays;

/** LSPlant method hooks and Object[] callbacks, independent of Android service classes. */
public final class MethodHook {
    public interface Around {
        Object invoke(Call call) throws Throwable;
    }
    interface Installer {
        void install(Method target, Around around) throws Exception;
    }

    interface NativeHook {
        Method hook(Method target, Object callback, Method method);
    }

    static void install(Method target, Around around, NativeHook nativeHook) throws Exception {
        target.setAccessible(true);
        MethodHook callback = new MethodHook(target, around);
        // Concurrent callbacks wait until the backup method is published.
        synchronized (callback) {
            Method backup = nativeHook.hook(
                    target, callback, MethodHook.class.getMethod("callback", Object[].class));
            if (backup == null)
                throw new IllegalStateException("Cannot hook " + target);
            callback.setBackup(backup);
        }
    }

    private final boolean isStatic;
    private final Around around;
    private Method backup;

    public MethodHook(Method target, Around around) {
        this.isStatic = Modifier.isStatic(target.getModifiers());
        this.around = around;
    }

    public synchronized void setBackup(Method backup) {
        this.backup = backup;
        backup.setAccessible(true);
    }

    public Object callback(Object[] arguments) throws Throwable {
        Method original;
        synchronized (this) {
            original = backup;
        }
        return around.invoke(new Call(original, arguments, isStatic));
    }

    public static final class Call {
        public final Object[] arguments;
        private final Method backup;
        private final boolean isStatic;
        private Call(Method backup, Object[] arguments, boolean isStatic) {
            this.backup = backup;
            this.arguments = arguments;
            this.isStatic = isStatic;
        }
        public Object original() throws Throwable {
            try {
                return isStatic ? backup.invoke(null, arguments)
                                : backup.invoke(arguments[0],
                                          Arrays.copyOfRange(arguments, 1, arguments.length));
            } catch (InvocationTargetException error) {
                throw error.getCause();
            }
        }
    }
}
