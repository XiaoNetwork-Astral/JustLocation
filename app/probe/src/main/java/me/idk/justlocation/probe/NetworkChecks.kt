package me.idk.justlocation.probe

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import android.net.wifi.WifiInfo
import android.net.wifi.WifiManager
import android.telephony.CellInfo
import android.telephony.SubscriptionManager
import android.telephony.TelephonyManager

/** Read telephony and Wi-Fi outputs as an ordinary application. */
internal class NetworkChecks(private val context: Context) {
    // Denied reads are observations in the report; this probe does not request extra permissions.
    @SuppressLint("MissingPermission")
    fun collect(report: CheckReport) {
        val telephony = context.getSystemService(TelephonyManager::class.java)
        report.section("cells")
        val cells: List<CellInfo>? =
            try {
                telephony.allCellInfo
            } catch (error: Exception) {
                null
            }
        if (cells == null) report.line("read failed (permission missing?)")
        else if (cells.isEmpty()) report.line("no cell info")
        else
            for (cell in cells.take(6)) {
                report.line(describe(cell))
            }

        report.section("sim")
        report.line(
            "network_operator ${telephony.networkOperatorName} / ${telephony.networkOperator}"
        )
        report.line("sim_operator ${telephony.simOperatorName} / ${telephony.simOperator}")
        report.line("sim_state ${telephony.simState}")
        val subscriptions = context.getSystemService(SubscriptionManager::class.java)
        val list =
            try {
                subscriptions?.activeSubscriptionInfoList
            } catch (error: Exception) {
                null
            }
        if (list == null)
            report.line("subscription list unreadable (needs READ_PHONE_STATE or more permission)")
        else
            for (info in list) {
                report.line(
                    "slot ${info.simSlotIndex} subscription ${info.subscriptionId} ${info.carrierName} " +
                        "${info.mccString}${info.mncString} ${info.countryIso}"
                )
            }

        report.section("wifi")
        // Keep all three read paths separate, including the legacy connection getter.
        val wifi = context.getSystemService(WifiManager::class.java)
        val info =
            try {
                wifi.connectionInfo
            } catch (error: Exception) {
                null
            }
        report.line(
            if (info == null) "read failed"
            else
                "SSID ${info.ssid} BSSID ${info.bssid} rssi ${info.rssi} " +
                    "state ${info.supplicantState} network ${info.networkId}"
        )
        val nearby =
            try {
                wifi.scanResults
            } catch (error: Exception) {
                null
            }
        report.line(
            when {
                nearby == null -> "nearby networks read failed"
                else ->
                    "nearby networks ${nearby.size}:" +
                        nearby.joinToString(" / ") { "${it.SSID}(${it.BSSID} ${it.level})" }
            }
        )
        val connectivity = context.getSystemService(ConnectivityManager::class.java)
        val active = connectivity?.activeNetwork
        val caps = if (active == null) null else connectivity?.getNetworkCapabilities(active)
        val transport = caps?.transportInfo
        report.line(
            if (transport is WifiInfo)
                "NetworkCapabilities ${transport.ssid} ${transport.bssid} rssi ${transport.rssi}"
            else
                "NetworkCapabilities has no WifiInfo (${transport?.javaClass?.simpleName ?: "no active network"})"
        )
    }

    private fun describe(cell: CellInfo): String {
        val identity = cell.cellIdentity
        val fields = listOf("mMcc", "mMnc", "mTac", "mCi", "mNci", "mLac", "mCid", "mPci")
        val values = fields.mapNotNull { name ->
            val value =
                try {
                    identity.javaClass.getMethod("get" + name.removePrefix("m")).invoke(identity)
                } catch (_: Exception) {
                    try {
                        identity.javaClass
                            .getDeclaredField(name)
                            .apply { isAccessible = true }
                            .get(identity)
                    } catch (_: Exception) {
                        null
                    }
                }
            value?.let { "${name.removePrefix("m")}=$it" }
        }
        return "${cell.javaClass.simpleName} registered=${cell.isRegistered} ${values.joinToString(" ")}"
    }
}
