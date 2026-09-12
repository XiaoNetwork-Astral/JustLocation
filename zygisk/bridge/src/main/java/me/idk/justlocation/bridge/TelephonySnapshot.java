package me.idk.justlocation.bridge;

import android.os.Parcel;
import android.telephony.CellIdentity;
import android.telephony.CellInfo;
import android.telephony.SignalStrength;
import android.telephony.SubscriptionInfo;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;

import org.json.JSONObject;

/** Immutable heartbeat snapshot shared by cell queries and listener adapters. */
final class TelephonySnapshot {
    private record Group(int subscriptionId, int slot, List<CellInfo> cells) {}
    private final SessionSnapshot scope;
    final boolean cellsEnabled, simEnabled;
    private final List<Group> groups;
    private final List<SubscriptionInfo> subscriptions;
    private TelephonySnapshot(SessionSnapshot scope, boolean cellsEnabled, boolean simEnabled,
            List<Group> groups, List<SubscriptionInfo> subscriptions) {
        this.scope = scope;
        this.cellsEnabled = cellsEnabled;
        this.simEnabled = simEnabled;
        this.groups = List.copyOf(groups);
        this.subscriptions = List.copyOf(subscriptions);
    }
    static TelephonySnapshot parse(String response, long nowMs) throws Exception {
        if (response == null)
            return null;
        JSONObject json = new JSONObject(response);
        if (json.getInt("version") != 1 || !json.getBoolean("ok"))
            return null;
        JSONObject state = json.getJSONObject("state");
        if (!state.getBoolean("requested_active") || state.isNull("telephony_output"))
            return null;
        JSONObject config = state.getJSONObject("telephony"),
                   output = state.getJSONObject("telephony_output");
        JSONObject selection = state.getJSONObject("config").getJSONObject("scope");
        String mode = selection.getString("mode");
        HashSet<String> packages = new HashSet<>();
        if (mode.equals("apps")) {
            var names = selection.getJSONArray("packages");
            for (int i = 0; i < names.length(); i++)
                packages.add(names.getString(i));
        } else if (!mode.equals("all"))
            return null;
        List<Group> groups = new ArrayList<>();
        List<SubscriptionInfo> subscriptions = new ArrayList<>();
        var groupList = output.getJSONArray("groups");
        for (int i = 0; i < groupList.length(); i++) {
            JSONObject group = groupList.getJSONObject(i);
            var cells = group.getJSONArray("cells");
            List<CellInfo> records = new ArrayList<>();
            for (int j = 0; j < cells.length(); j++)
                records.add(TelephonyObjects.cell(cells.getJSONObject(j), 0));
            groups.add(new Group(
                    group.getInt("subscription_id"), group.getInt("slot"), List.copyOf(records)));
        }
        var subs = output.getJSONArray("subscriptions");
        for (int i = 0; i < subs.length(); i++)
            subscriptions.add(TelephonyObjects.subscription(subs.getJSONObject(i)));
        return new TelephonySnapshot(new SessionSnapshot(true, mode.equals("all"), packages, nowMs),
                config.getBoolean("cells_enabled"), config.getBoolean("sim_enabled"), groups,
                subscriptions);
    }
    boolean appliesTo(String packageName, long nowMs) {
        return scope.appliesTo(packageName, nowMs);
    }
    int resolveSubscription(int subscriptionId, int slot) {
        if (subscriptionId >= 0 && subscriptionId != Integer.MAX_VALUE)
            return subscriptionId;
        for (Group group : groups)
            if (group.slot == slot)
                return group.subscriptionId;
        return -1;
    }
    List<CellInfo> cells(int subscriptionId, long elapsedNanos) throws Exception {
        List<CellInfo> result = new ArrayList<>();
        for (Group group : groups)
            if (subscriptionId == Integer.MAX_VALUE || group.subscriptionId == subscriptionId) {
                for (CellInfo cell : group.cells) {
                    Parcel parcel = Parcel.obtain();
                    try {
                        cell.writeToParcel(parcel, 0);
                        parcel.setDataPosition(0);
                        CellInfo copy = CellInfo.CREATOR.createFromParcel(parcel);
                        CellInfo.class.getMethod("setTimeStamp", long.class)
                                .invoke(copy, elapsedNanos);
                        result.add(copy);
                    } finally {
                        parcel.recycle();
                    }
                }
            }
        return result;
    }
    CellIdentity identity(int subscriptionId, long elapsedNanos) throws Exception {
        for (CellInfo cell : cells(subscriptionId, elapsedNanos))
            if (cell.isRegistered())
                return cell.getCellIdentity();
        return TelephonyObjects.emptyIdentity();
    }
    SignalStrength signal(int subscriptionId, long elapsedNanos) throws Exception {
        for (CellInfo cell : cells(subscriptionId, elapsedNanos))
            if (cell.isRegistered())
                return TelephonyObjects.signal(cell.getCellSignalStrength());
        return TelephonyObjects.signal(null);
    }
    android.telephony.ServiceState serviceState(
            int subscriptionId, android.telephony.ServiceState original) throws Exception {
        CellIdentity serving = null;
        for (CellInfo cell : cells(subscriptionId, 0))
            if (cell.isRegistered()) {
                serving = cell.getCellIdentity();
                break;
            }
        String carrier = null;
        if (simEnabled)
            for (SubscriptionInfo sub : subscriptions)
                if (sub.getSubscriptionId() == subscriptionId)
                    carrier = sub.getCarrierName().toString();
        return ServiceStateObjects.replace(original, serving, carrier);
    }
    List<SubscriptionInfo> subscriptions() {
        return new ArrayList<>(subscriptions);
    }
    SubscriptionInfo subscription(SubscriptionInfo original) throws Exception {
        for (SubscriptionInfo replacement : subscriptions) {
            if (original.getSubscriptionId() == replacement.getSubscriptionId()
                    && original.getSimSlotIndex() == replacement.getSimSlotIndex())
                return TelephonyObjects.operator(original, replacement);
        }
        return original;
    }
}
