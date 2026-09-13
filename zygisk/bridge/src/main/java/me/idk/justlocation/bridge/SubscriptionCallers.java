package me.idk.justlocation.bridge;

import android.content.Context;
import android.os.Binder;

import java.lang.reflect.Method;
import java.util.Arrays;

/** Verify Binder ownership before using names; virtual records need general phone permission. */
final class SubscriptionCallers {
    private final Context context;
    private final Method readPhoneState;

    SubscriptionCallers(Context context, ClassLoader loader) throws Exception {
        this.context = context;
        readPhoneState =
                Class.forName("com.android.internal.telephony.TelephonyPermissions", false, loader)
                        .getDeclaredMethod("checkCallingOrSelfReadPhoneState", Context.class,
                                int.class, String.class, String.class, String.class);
    }
    String[] packages(String claimed) {
        int uid = Binder.getCallingUid();
        // Framework and local phone work must always see the physical subscription database.
        if (uid % 100000 < 10000 || uid == android.os.Process.myUid())
            return null;
        String[] owned = context.getPackageManager().getPackagesForUid(uid);
        if (owned == null || owned.length == 0)
            return null;
        if (claimed == null)
            return owned;
        return Arrays.asList(owned).contains(claimed) ? new String[] {claimed} : null;
    }
    boolean canReadVirtual(String claimed, String attribution) {
        if (claimed == null || packages(claimed) == null)
            return false;
        try {
            // INVALID_SUBSCRIPTION_ID prevents a real card's carrier privileges granting a virtual
            // one.
            return (boolean) readPhoneState.invoke(
                    null, context, -1, claimed, attribution, "JustLocation virtual subscription");
        } catch (ReflectiveOperationException | SecurityException denied) {
            return false;
        }
    }
}
