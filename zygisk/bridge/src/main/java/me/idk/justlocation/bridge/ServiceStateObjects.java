package me.idk.justlocation.bridge;

import android.telephony.*;

import java.util.List;

/** Copy the caller's authorized state, preserving identity and operator redactions. */
final class ServiceStateObjects {
    private ServiceStateObjects() {}
    static ServiceState replace(ServiceState original, CellIdentity serving, String carrier)
            throws Exception {
        ServiceState result = new ServiceState(original);
        String numeric = serving == null
                ? ""
                : (String) CellIdentity.class.getMethod("getPlmn").invoke(serving);
        if (numeric == null)
            numeric = "";
        result.setOperatorName(original.getOperatorAlphaLong() == null ? null : carrier,
                original.getOperatorAlphaShort() == null ? null : carrier,
                original.getOperatorNumeric() == null ? null : numeric);
        // AOSP's location sanitizer leaves raw alpha names and radio metadata intact.
        call(result, "setOperatorAlphaLongRaw", String.class,
                original.getOperatorNumeric() == null
                                || ServiceState.class.getMethod("getOperatorAlphaLongRaw")
                                                .invoke(original)
                                        == null
                        ? null
                        : carrier);
        call(result, "setOperatorAlphaShortRaw", String.class,
                original.getOperatorNumeric() == null
                                || ServiceState.class.getMethod("getOperatorAlphaShortRaw")
                                                .invoke(original)
                                        == null
                        ? null
                        : carrier);
        call(result, "setChannelNumber", int.class,
                serving == null ? CellInfo.UNAVAILABLE
                                : CellIdentity.class.getMethod("getChannelNumber").invoke(serving));
        call(result, "setCellBandwidths", int[].class, new int[0]);
        call(result, "setNrFrequencyRange", int.class, 0);
        call(result, "setArfcnRsrpBoost", int.class, 0);
        boolean coarseVisible = original.getOperatorNumeric() != null;
        int sid = coarseVisible && serving instanceof CellIdentityCdma c ? c.getSystemId() : -1;
        int nid = coarseVisible && serving instanceof CellIdentityCdma c ? c.getNetworkId() : -1;
        ServiceState.class.getMethod("setCdmaSystemAndNetworkId", int.class, int.class)
                .invoke(result, sid, nid);
        @SuppressWarnings("unchecked")
        List<NetworkRegistrationInfo> registrations =
                (List<NetworkRegistrationInfo>)
                        ServiceState.class.getMethod("getNetworkRegistrationInfoList")
                                .invoke(original);
        for (NetworkRegistrationInfo registration : registrations) {
            if (registration.getTransportType() != AccessNetworkConstants.TRANSPORT_TYPE_WWAN)
                continue;
            Class<?> builderType =
                    Class.forName("android.telephony.NetworkRegistrationInfo$Builder");
            Object builder = builderType.getConstructor(NetworkRegistrationInfo.class)
                                     .newInstance(registration);
            builderType.getMethod("setCellIdentity", CellIdentity.class)
                    .invoke(builder, registration.getCellIdentity() == null ? null : serving);
            builderType.getMethod("setAccessNetworkTechnology", int.class)
                    .invoke(builder, technology(serving));
            builderType.getMethod("setRegisteredPlmn", String.class)
                    .invoke(builder, coarseVisible ? numeric : "");
            NetworkRegistrationInfo replacement =
                    (NetworkRegistrationInfo) builderType.getMethod("build").invoke(builder);
            // Retaining modem EN-DC availability here would describe the real serving network.
            NetworkRegistrationInfo.class.getMethod("setNrState", int.class).invoke(replacement, 0);
            ServiceState
                    .class.getMethod("addNetworkRegistrationInfo", NetworkRegistrationInfo.class)
                    .invoke(result, replacement);
        }
        return result;
    }
    private static int technology(CellIdentity identity) {
        if (identity instanceof CellIdentityGsm)
            return TelephonyManager.NETWORK_TYPE_GSM;
        if (identity instanceof CellIdentityWcdma)
            return TelephonyManager.NETWORK_TYPE_UMTS;
        if (identity instanceof CellIdentityLte)
            return TelephonyManager.NETWORK_TYPE_LTE;
        if (identity instanceof CellIdentityNr)
            return TelephonyManager.NETWORK_TYPE_NR;
        if (identity instanceof CellIdentityCdma)
            return TelephonyManager.NETWORK_TYPE_CDMA;
        return TelephonyManager.NETWORK_TYPE_UNKNOWN;
    }
    private static void call(ServiceState state, String name, Class<?> type, Object value)
            throws Exception {
        ServiceState.class.getMethod(name, type).invoke(state, value);
    }
}
