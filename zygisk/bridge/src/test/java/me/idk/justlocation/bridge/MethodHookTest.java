package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import org.junit.Test;

public class MethodHookTest {
    @Test
    public void installerPublishesBackupForTheInstalledCallback() throws Throwable {
        MethodHook[] installed = new MethodHook[1];
        var method = Target.class.getMethod("add", int.class);
        var callbackMethod = MethodHook.class.getMethod("callback", Object[].class);
        MethodHook.install(
                method, call -> (int) call.original() * 2, (target, receiver, callback) -> {
                    assertSame(method, target);
                    assertEquals(callbackMethod, callback);
                    installed[0] = (MethodHook) receiver;
                    return target;
                });
        assertEquals(24, installed[0].callback(new Object[] {new Target(), 5}));
    }

    @Test
    public void installerRejectsMissingBackup() throws Exception {
        var method = Target.class.getMethod("add", int.class);
        assertThrows(IllegalStateException.class,
                ()
                        -> MethodHook.install(method, MethodHook.Call::original,
                                (target, receiver, callback) -> null));
    }

    public static class Target {
        public int value = 7;
        public int add(int amount) {
            return value + amount;
        }
        public static String echo(String value) {
            return value;
        }
        public void fail() {
            throw new IllegalStateException("original failure");
        }
    }

    @Test
    public void instanceBackupReceivesOriginalReceiverAndArguments() throws Throwable {
        var method = Target.class.getMethod("add", int.class);
        var hook = new MethodHook(method, MethodHook.Call::original);
        hook.setBackup(method);
        assertEquals(12, hook.callback(new Object[] {new Target(), 5}));
    }

    @Test
    public void staticBackupDoesNotDropItsFirstArgument() throws Throwable {
        var method = Target.class.getMethod("echo", String.class);
        var hook = new MethodHook(method, MethodHook.Call::original);
        hook.setBackup(method);
        assertEquals("first argument", hook.callback(new Object[] {"first argument"}));
    }

    @Test
    public void originalExceptionsAreNotWrappedOrSwallowed() throws Throwable {
        var method = Target.class.getMethod("fail");
        var hook = new MethodHook(method, MethodHook.Call::original);
        hook.setBackup(method);
        try {
            hook.callback(new Object[] {new Target()});
            fail("expected original failure");
        } catch (IllegalStateException expected) {
            assertEquals("original failure", expected.getMessage());
        }
    }
}
