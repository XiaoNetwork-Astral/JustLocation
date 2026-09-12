package me.idk.justlocation.bridge;

import org.junit.Test;
import static org.junit.Assert.*;

public class MethodHookTest {
    public static class Target {
        public int value = 7;
        public int add(int amount) { return value + amount; }
        public static String echo(String value) { return value; }
        public void fail() { throw new IllegalStateException("original failure"); }
    }

    @Test public void instanceBackupReceivesOriginalReceiverAndArguments() throws Throwable {
        var method = Target.class.getMethod("add", int.class);
        var hook = new MethodHook(method, MethodHook.Call::original);
        hook.setBackup(method);
        assertEquals(12, hook.callback(new Object[]{new Target(), 5}));
    }

    @Test public void staticBackupDoesNotDropItsFirstArgument() throws Throwable {
        var method = Target.class.getMethod("echo", String.class);
        var hook = new MethodHook(method, MethodHook.Call::original);
        hook.setBackup(method);
        assertEquals("first argument", hook.callback(new Object[]{"first argument"}));
    }

    @Test public void originalExceptionsAreNotWrappedOrSwallowed() throws Throwable {
        var method = Target.class.getMethod("fail");
        var hook = new MethodHook(method, MethodHook.Call::original);
        hook.setBackup(method);
        try { hook.callback(new Object[]{new Target()}); fail("expected original failure"); }
        catch (IllegalStateException expected) { assertEquals("original failure", expected.getMessage()); }
    }
}
