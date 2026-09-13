package me.idk.justlocation.bridge;

import android.app.Instrumentation;
import android.os.Bundle;
import android.os.Parcel;
import android.telephony.*;

import java.util.List;

import org.json.*;

/** Runs against real Android framework objects on the dedicated API 35 AVD. */
public final class TelephonyObjectChecks extends Instrumentation {
    private boolean scopeOnly;
    @Override
    public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        scopeOnly = arguments != null && "true".equals(arguments.getString("scope_only"));
        start();
    }
    @Override
    public void onStart() {
        Bundle result = new Bundle();
        try {
            String[] identities = {
                    "{\"radio\":\"gsm\",\"mcc\":\"460\",\"mnc\":\"01\",\"lac\":1,\"cid\":11}",
                    "{\"radio\":\"wcdma\",\"mcc\":\"460\",\"mnc\":\"01\",\"lac\":2,\"cid\":268435455}",
                    "{\"radio\":\"lte\",\"mcc\":\"460\",\"mnc\":\"001\",\"tac\":3,\"ci\":268435455}",
                    "{\"radio\":\"nr\",\"mcc\":\"460\",\"mnc\":\"001\",\"tac\":16777215,\"nci\":"
                            + "68719476735}",
                    "{\"radio\":\"cdma\",\"sid\":32767,\"nid\":65535,\"bid\":65535}"};
            JSONArray cells = new JSONArray();
            for (String identity : identities)
                cells.put(new JSONObject()
                                .put("identity", new JSONObject(identity))
                                .put("position",
                                        new JSONObject()
                                                .put("latitude", 1.25)
                                                .put("longitude", -2.5))
                                .put("registered", cells.length() == 0)
                                .put("dbm", -90));
            JSONObject state =
                    new JSONObject()
                            .put("requested_active", true)
                            .put("config",
                                    new JSONObject().put("scope",
                                            new JSONObject()
                                                    .put("mode", "apps")
                                                    .put("packages",
                                                            new JSONArray().put(
                                                                    "example.selected"))))
                            .put("telephony",
                                    new JSONObject()
                                            .put("cells_enabled", true)
                                            .put("sim_enabled", true))
                            .put("telephony_output",
                                    new JSONObject()
                                            .put("groups",
                                                    new JSONArray().put(new JSONObject()
                                                                    .put("subscription_id", 7)
                                                                    .put("slot", 0)
                                                                    .put("cells", cells)))
                                            .put("subscriptions",
                                                    new JSONArray().put(new JSONObject()
                                                                    .put("id", 7)
                                                                    .put("slot", 0)
                                                                    .put("mcc", "460")
                                                                    .put("mnc", "001")
                                                                    .put("country", "cn")
                                                                    .put("carrier", "Test"))));
            JSONObject response =
                    new JSONObject().put("version", 1).put("ok", true).put("state", state);
            TelephonySnapshot snapshot = TelephonySnapshot.parse(response.toString(), 1000);
            check(snapshot != null && snapshot.cellsApplyTo("example.selected", 1001),
                    "selected scope");
            check(!snapshot.cellsApplyTo("example.other", 1001)
                            && !snapshot.cellsApplyTo("example.selected", 21_000),
                    "scope and expiry");
            if (scopeOnly) {
                checkServiceState(snapshot);
                state.put("scopes",
                        new JSONObject().put("sim",
                                new JSONObject()
                                        .put("mode", "apps")
                                        .put("packages", new JSONArray().put("example.sim"))));
                TelephonySnapshot split = TelephonySnapshot.parse(response.toString(), 1000);
                check(split.cellsApplyTo("example.selected", 1001)
                                && !split.simAppliesTo("example.selected", 1001),
                        "cell-only scope");
                check(split.simAppliesTo("example.sim", 1001)
                                && !split.cellsApplyTo("example.sim", 1001),
                        "SIM-only scope");
                check(!split.simAppliesTo("example.sim", 21_000) && !split.simAppliesTo(null, 1001),
                        "SIM expiry and unknown caller");
                state.put("scopes", new JSONObject());
                check(!TelephonySnapshot.parse(response.toString(), 1000)
                                .simAppliesTo("example.selected", 1001),
                        "missing new scope does not inherit");
                state.put("requested_active", false);
                check(TelephonySnapshot.parse(response.toString(), 1000) == null, "stop");
                result.putString("stream",
                        "PASS: independent cell/SIM scopes, legacy fallback, malformed scope, expiry, unknown caller, ServiceState isolation/redaction/parcel, stop\n");
                finish(-1, result);
                return;
            }
            List<CellInfo> output = snapshot.cells(7, 1234567890L);
            check(output.size() == 5, "five radio types");
            for (CellInfo cell : output) {
                check(cell.getTimeStamp() == 1234567890L, "elapsed timestamp");
                check(cell.getCellSignalStrength().getDbm() == -90, "signal dBm " + cell);
                Parcel parcel = Parcel.obtain();
                try {
                    cell.writeToParcel(parcel, 0);
                    parcel.setDataPosition(0);
                    check(cell.equals(CellInfo.CREATOR.createFromParcel(parcel)),
                            "binder parcel round trip");
                } finally {
                    parcel.recycle();
                }
            }
            check(((CellInfoGsm) output.get(0)).getCellIdentity().getArfcn()
                            == CellInfo.UNAVAILABLE,
                    "unknown ARFCN");
            check(((CellInfoLte) output.get(2)).getCellIdentity().getPci() == CellInfo.UNAVAILABLE,
                    "unknown PCI");
            check(((CellInfoLte) output.get(2)).getCellIdentity().getMncString().equals("001"),
                    "MNC width");
            check(((CellIdentityNr) output.get(3).getCellIdentity()).getNci() == 68719476735L,
                    "36 bit NCI");
            check(((CellInfoCdma) output.get(4)).getCellIdentity().getLatitude() == 18000,
                    "CDMA quarter arc seconds");
            check(output.get(0).isRegistered() && !output.get(1).isRegistered(), "registration");
            output.clear();
            check(snapshot.cells(7, 2).size() == 5, "fresh list");
            check(snapshot.cells(999, 2).isEmpty(), "unknown subscription");
            check(snapshot.subscriptions().get(0).getMncString().equals("001"), "SIM MNC");
            SubscriptionInfo original = TelephonyObjects.subscription(new JSONObject()
                            .put("id", 7)
                            .put("slot", 0)
                            .put("mcc", "310")
                            .put("mnc", "260")
                            .put("country", "us")
                            .put("carrier", "Original"));
            SubscriptionInfo replaced = snapshot.subscription(original);
            check(replaced != original && replaced.getMncString().equals("001"),
                    "SIM operator replacement");
            check(replaced.getSubscriptionId() == 7 && replaced.getSimSlotIndex() == 0,
                    "SIM routing identity preserved");
            check(replaced.getIccId().equals(original.getIccId())
                            && replaced.getNumber().equals(original.getNumber()),
                    "SIM redaction preserved");
            check(original.getMncString().equals("260"), "SIM original not mutated");
            Parcel simParcel = Parcel.obtain();
            try {
                replaced.writeToParcel(simParcel, 0);
                simParcel.setDataPosition(0);
                check(replaced.equals(SubscriptionInfo.CREATOR.createFromParcel(simParcel)),
                        "SIM parcel round trip");
            } finally {
                simParcel.recycle();
            }
            JSONArray detected =
                    new JSONArray(new String(PhoneSubscriptions.encode(List.of(original)),
                            java.nio.charset.StandardCharsets.UTF_8));
            check(detected.length() == 1 && detected.getJSONObject(0).length() == 6,
                    "only six public card fields collected");
            check(detected.getJSONObject(0).getInt("id") == 7
                            && detected.getJSONObject(0).getString("mnc").equals("260"),
                    "actual card mapping");
            SubscriptionInfo unicode = TelephonyObjects.subscription(new JSONObject()
                            .put("id", 8)
                            .put("slot", 1)
                            .put("mcc", "460")
                            .put("mnc", "001")
                            .put("country", "cn")
                            .put("carrier", "测试📱"));
            JSONArray unicodeCards =
                    new JSONArray(new String(PhoneSubscriptions.encode(List.of(unicode, original)),
                            java.nio.charset.StandardCharsets.UTF_8));
            check(unicodeCards.getJSONObject(1).getString("carrier").equals("测试📱"),
                    "card UTF-8 and slot order");
            check(PhoneSubscriptions.encode(null) == null, "missing card read");
            check(snapshot.resolveSubscription(Integer.MAX_VALUE, 0) == 7,
                    "default subscription follows slot");
            check(snapshot.resolveSubscription(Integer.MAX_VALUE, 1) == -1,
                    "missing slot stays empty");
            check(snapshot.signal(7, 2).getCellSignalStrengths().get(0).getDbm() == -90,
                    "combined serving signal");
            check(snapshot.identity(7, 2).equals(snapshot.cells(7, 2).get(0).getCellIdentity()),
                    "serving identity");
            checkServiceState(snapshot);
            ClassLoader phoneLoader =
                    getContext()
                            .createPackageContext("com.android.phone",
                                    android.content.Context.CONTEXT_INCLUDE_CODE
                                            | android.content.Context.CONTEXT_IGNORE_SECURITY)
                            .getClassLoader();
            new TelephonyQueries(
                    Class.forName("com.android.phone.PhoneInterfaceManager", false, phoneLoader),
                    Class.forName("com.android.internal.telephony.Phone", false, phoneLoader),
                    Class.forName("android.os.WorkSource", false, phoneLoader),
                    Class.forName("android.telephony.ICellInfoCallback", false, phoneLoader),
                    pkg -> null);
            new SubscriptionQueries(
                    Class.forName(
                            "com.android.internal.telephony.subscription.SubscriptionManagerService",
                            false, phoneLoader),
                    pkg -> null);
            new ServiceStateQueries(
                    Class.forName("com.android.phone.PhoneInterfaceManager", false, phoneLoader),
                    (pkg, slot, value) -> value);
            ClassLoader servicesLoader = new dalvik.system.PathClassLoader(
                    "/system/framework/services.jar", ClassLoader.getSystemClassLoader());
            new TelephonyRegistryAdapter(
                    Class.forName("com.android.server.TelephonyRegistry", false, servicesLoader),
                    Class.forName(
                            "com.android.server.TelephonyRegistry$Record", false, servicesLoader),
                    Class.forName("com.android.internal.telephony.IPhoneStateListener", false,
                            servicesLoader),
                    TelephonyCallback.class, (pkg, sub, slot, callback, authorized) -> null);
            state.put("requested_active", false);
            check(TelephonySnapshot.parse(response.toString(), 1000) == null, "stop");
            result.putString("stream",
                    "PASS: five radios, parcel round trips, NR ID, missing metadata, SIM operator and "
                            + "redaction, scope, expiry, stop, serving signal, ServiceState "
                            + "identity/operator/redaction, phone, subscription and registry signatures\n");
            finish(-1, result);
        } catch (Throwable error) {
            result.putString("stream", android.util.Log.getStackTraceString(error));
            finish(1, result);
        }
    }
    private static void check(boolean value, String message) {
        if (!value)
            throw new AssertionError(message);
    }
    private static void checkServiceState(TelephonySnapshot snapshot) throws Exception {
        CellIdentity real = snapshot.cells(7, 0).get(2).getCellIdentity();
        ServiceState original = new ServiceState();
        original.setOperatorName("Real", "Real", "310260");
        Class<?> builderType = Class.forName("android.telephony.NetworkRegistrationInfo$Builder");
        Object builder = builderType.getConstructor().newInstance();
        builderType.getMethod("setDomain", int.class).invoke(builder, 2);
        builderType.getMethod("setTransportType", int.class).invoke(builder, 1);
        builderType.getMethod("setRegistrationState", int.class).invoke(builder, 1);
        builderType.getMethod("setCellIdentity", CellIdentity.class).invoke(builder, real);
        builderType.getMethod("setRegisteredPlmn", String.class).invoke(builder, "310260");
        Object registration = builderType.getMethod("build").invoke(builder);
        ServiceState.class.getMethod("addNetworkRegistrationInfo", NetworkRegistrationInfo.class)
                .invoke(original, registration);
        ServiceState changed = snapshot.serviceState(7, original, true, true);
        check(changed.getOperatorNumeric().equals("46001"), "ServiceState serving PLMN");
        check(changed.getOperatorAlphaLong().equals("Test"), "ServiceState configured carrier");
        check(registration(changed).getCellIdentity().equals(snapshot.identity(7, 0)),
                "ServiceState serving identity");
        check(registration(original).getCellIdentity().equals(real)
                        && original.getOperatorNumeric().equals("310260"),
                "ServiceState original untouched");
        for (boolean coarse : new boolean[] {false, true}) {
            ServiceState sanitized =
                    (ServiceState) ServiceState
                            .class.getMethod("createLocationInfoSanitizedCopy", boolean.class)
                            .invoke(original, coarse);
            ServiceState replaced = snapshot.serviceState(7, sanitized, true, true);
            check(registration(replaced).getCellIdentity() == null,
                    "ServiceState fine redaction preserved");
            check(coarse ? replaced.getOperatorNumeric() == null
                         : replaced.getOperatorNumeric().equals("46001"),
                    "ServiceState coarse redaction preserved");
        }
        ServiceState cellsOnly = snapshot.serviceState(7, original, true, false);
        check(cellsOnly.getOperatorAlphaLong().equals("Real")
                        && cellsOnly.getOperatorNumeric().equals("46001"),
                "cells cannot change out-of-scope carrier");
        ServiceState simOnly = snapshot.serviceState(7, original, false, true);
        check(simOnly.getOperatorAlphaLong().equals("Test")
                        && simOnly.getOperatorNumeric().equals("310260")
                        && registration(simOnly).getCellIdentity().equals(real),
                "SIM cannot change out-of-scope cell identity");
        ServiceState empty = snapshot.serviceState(999, original, true, true);
        check(registration(empty).getCellIdentity() == null && empty.getOperatorNumeric().isEmpty(),
                "no real cell outside coverage");
        Parcel parcel = Parcel.obtain();
        try {
            changed.writeToParcel(parcel, 0);
            parcel.setDataPosition(0);
            check(changed.equals(ServiceState.CREATOR.createFromParcel(parcel)),
                    "ServiceState parcel round trip");
        } finally {
            parcel.recycle();
        }
    }
    @SuppressWarnings("unchecked")
    private static NetworkRegistrationInfo registration(ServiceState state) throws Exception {
        return ((List<NetworkRegistrationInfo>)
                        ServiceState.class.getMethod("getNetworkRegistrationInfoList")
                                .invoke(state))
                .get(0);
    }
}
