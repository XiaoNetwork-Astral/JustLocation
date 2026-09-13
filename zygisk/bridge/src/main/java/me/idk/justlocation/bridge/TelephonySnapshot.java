package me.idk.justlocation.bridge;

import android.os.Parcel;
import android.telephony.CellIdentity;
import android.telephony.CellInfo;
import android.telephony.SignalStrength;
import android.telephony.SubscriptionInfo;

import java.util.ArrayList;
import java.util.List;

import org.json.JSONObject;

/** Immutable heartbeat snapshot shared by cell queries and listener adapters. */
final class TelephonySnapshot {
    private record Group(int subscriptionId, int slot, List<CellInfo> cells) {}
    private final SessionSnapshot cellsScope, simScope;
    final boolean cellsEnabled, simEnabled;
    private final List<Group> groups;
    private final List<SubscriptionInfo> subscriptions;
    private final VirtualSubscriptions virtuals;
    private final Object virtualVersion;
    private String publication;
    private record VirtualVersion(ScopeSelection scope, VirtualSubscriptions.Version model) {}
    private TelephonySnapshot(SessionSnapshot cellsScope, SessionSnapshot simScope,
            boolean cellsEnabled, boolean simEnabled, List<Group> groups,
            List<SubscriptionInfo> subscriptions, VirtualSubscriptions virtuals,
            Object virtualVersion) {
        this.cellsScope = cellsScope;
        this.simScope = simScope;
        this.cellsEnabled = cellsEnabled;
        this.simEnabled = simEnabled;
        this.groups = List.copyOf(groups);
        this.subscriptions = List.copyOf(subscriptions);
        this.virtuals = virtuals;
        this.virtualVersion = virtualVersion;
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
        ScopeSelection cellsScope = ScopeSelection.read(state, null);
        ScopeSelection simScope = ScopeSelection.read(state, "sim");
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
        List<VirtualSubscriptions.Entry> virtualEntries = new ArrayList<>();
        var virtualIds = output.optJSONArray("virtual_ids");
        if (virtualIds != null && state.optBoolean("virtual_sim_query_hook_ready", false))
            for (int i = 0; i < virtualIds.length(); i++) {
                int id = virtualIds.getInt(i);
                SubscriptionInfo info = subscriptions.stream()
                                                .filter(s -> s.getSubscriptionId() == id)
                                                .findFirst()
                                                .orElseThrow();
                virtualEntries.add(
                        new VirtualSubscriptions.Entry(id, info.getSimSlotIndex(), info));
            }
        VirtualSubscriptions virtuals = new VirtualSubscriptions(virtualEntries,
                virtualEntries.isEmpty() ? -1 : output.optInt("virtual_default_id", -1),
                output.optBoolean("has_real_subscriptions", true));
        TelephonySnapshot result = new TelephonySnapshot(
                cellsScope == null ? null : cellsScope.snapshot(nowMs),
                simScope == null ? null : simScope.snapshot(nowMs),
                config.getBoolean("cells_enabled"), config.getBoolean("sim_enabled"), groups,
                subscriptions, virtuals, new VirtualVersion(simScope, virtuals.version()));
        result.publication = state.optString("virtual_sim_version", "off");
        return result;
    }
    String publication(long nowMs) {
        return virtualVersion(nowMs) == null ? "off" : publication;
    }
    boolean cellsApplyTo(String packageName, long nowMs) {
        return cellsEnabled && cellsScope != null && cellsScope.appliesTo(packageName, nowMs);
    }
    boolean simAppliesTo(String packageName, long nowMs) {
        return simEnabled && simScope != null && simScope.appliesTo(packageName, nowMs);
    }
    VirtualSubscriptions virtuals(String packageName, long nowMs) {
        return !virtuals.isEmpty() && simAppliesTo(packageName, nowMs) ? virtuals : null;
    }
    VirtualSubscriptions virtuals(String[] packages, long nowMs) {
        if (packages == null || packages.length == 0 || virtuals.isEmpty())
            return null;
        for (String pkg : packages)
            if (!simAppliesTo(pkg, nowMs))
                return null;
        return virtuals;
    }
    Object virtualVersion(long nowMs) {
        return simEnabled && !virtuals.isEmpty() && simScope != null && simScope.fresh(nowMs)
                ? virtualVersion
                : null;
    }
    int resolveSubscription(int subscriptionId, int slot) {
        if (subscriptionId >= 0 && subscriptionId != Integer.MAX_VALUE)
            return subscriptionId;
        for (Group group : groups)
            if (group.slot == slot)
                return group.subscriptionId;
        for (SubscriptionInfo sub : subscriptions)
            if (sub.getSimSlotIndex() == slot)
                return sub.getSubscriptionId();
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
    android.telephony.ServiceState serviceState(int subscriptionId,
            android.telephony.ServiceState original, boolean replaceCells, boolean replaceSim)
            throws Exception {
        CellIdentity serving = null;
        for (CellInfo cell : replaceCells ? cells(subscriptionId, 0) : List.<CellInfo>of())
            if (cell.isRegistered()) {
                serving = cell.getCellIdentity();
                break;
            }
        String carrier = null;
        if (replaceSim)
            for (SubscriptionInfo sub : subscriptions)
                if (sub.getSubscriptionId() == subscriptionId)
                    carrier = sub.getCarrierName().toString();
        return ServiceStateObjects.replace(
                original, serving, carrier, replaceCells, replaceSim && carrier != null);
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
