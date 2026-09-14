package me.idk.justlocation.probe

import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.CancellationSignal
import android.os.Handler
import android.os.SystemClock
import android.telephony.CellIdentityCdma
import android.telephony.CellIdentityGsm
import android.telephony.CellIdentityLte
import android.telephony.CellIdentityNr
import android.telephony.CellIdentityWcdma
import android.telephony.CellInfo
import android.telephony.TelephonyManager
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.json.JSONArray
import org.json.JSONObject

/**
 * One measured place: a real fix plus the radio environment read at the same moment.
 *
 * A mock fix and a simulated environment are never mixed into this snapshot. The backend rejects a
 * snapshot whose `simulated` flag is true, and a fix that reports itself as mock is dropped here.
 */
internal class EnvironmentCollector(private val context: Context, private val handler: Handler) {
    @SuppressLint("MissingPermission")
    fun collect(destination: File, timeoutMs: Long = 12000) {
        val manager = context.getSystemService(LocationManager::class.java)
        val (fix, cached) = currentFix(manager, timeoutMs)
        val mockActive = fix?.isMock == true || anyMockFix(manager)
        val snapshot =
            JSONObject()
                .put("version", 1)
                .put("captured_ms", System.currentTimeMillis())
                .put("simulated", mockActive)
                .put("coordinate_system", "wgs84")
                .put("location", location(fix, cached))
                .put("cells", cells())
                .put("wifi", wifi())
                .put("capture", capture(mockActive))
        // A single write replaces the previous snapshot; readers only see complete documents.
        destination.writeText(snapshot.toString())
    }

    private fun location(fix: Location?, cached: Boolean): JSONObject {
        val location = JSONObject()
        if (fix == null) {
            // No fix at all is reported as a last-known placeholder and rejected by the backend.
            location.put("latitude", 0.0)
                .put("longitude", 0.0)
                .put("from_last_known", true)
            return location
        }
        location
            .put("latitude", fix.latitude)
            .put("longitude", fix.longitude)
            .put("altitude", fix.altitude)
            .put("accuracy", fix.accuracy.toDouble())
            .put("speed", fix.speed.toDouble())
            .put("bearing", fix.bearing.toDouble())
            .put("provider", fix.provider ?: "unknown")
            .put("from_last_known", cached)
            .put("fix_time_ms", fix.time)
            .put("fix_elapsed_ms", fix.elapsedRealtimeNanos / 1_000_000)
            .put("received_elapsed_ms", SystemClock.elapsedRealtime())
            .put("mock", fix.isMock)
        fix.extras?.let { extras ->
            val satellites = extras.getInt("satellites", -1)
            location.put("extras_satellites", satellites)
        }
        return location
    }

    /** Prefer a requested fix; fall back to the newest known one and report it as cached. */
    @SuppressLint("MissingPermission")
    private fun currentFix(manager: LocationManager?, timeoutMs: Long): Pair<Location?, Boolean> {
        if (manager == null) return null to true
        val providers = mutableListOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)
        if (LocationManager.FUSED_PROVIDER in manager.allProviders)
            providers.add(LocationManager.FUSED_PROVIDER)
        val fixes = java.util.concurrent.ConcurrentHashMap<String, Location>()
        val latch = CountDownLatch(providers.size)
        val cancellations = ArrayList<CancellationSignal>()
        for (provider in providers) {
            try {
                val cancellation = CancellationSignal()
                cancellations.add(cancellation)
                manager.getCurrentLocation(
                    provider,
                    cancellation,
                    { runnable -> handler.post(runnable) },
                ) { found ->
                    if (found != null) fixes[provider] = found
                    latch.countDown()
                }
            } catch (_: Exception) {
                latch.countDown()
            }
        }
        try {
            latch.await(timeoutMs, TimeUnit.MILLISECONDS)
        } finally {
            cancellations.forEach { it.cancel() }
        }
        // A fix that reports itself as mock is never stored as a measured place.
        fixes.values.filterNot { it.isMock }.maxByOrNull { it.elapsedRealtimeNanos }?.let {
            return it to false
        }
        val known =
            providers
                .mapNotNull { provider ->
                    try {
                        manager.getLastKnownLocation(provider)
                    } catch (_: Exception) {
                        null
                    }
                }
                .maxByOrNull { it.elapsedRealtimeNanos }
        return known to true
    }

    @SuppressLint("MissingPermission")
    private fun anyMockFix(manager: LocationManager?): Boolean {
        if (manager == null) return false
        for (provider in listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)) {
            val known =
                try {
                    manager.getLastKnownLocation(provider)
                } catch (_: Exception) {
                    null
                }
            if (known?.isMock == true) return true
        }
        return false
    }

    @SuppressLint("MissingPermission")
    private fun cells(): JSONArray {
        val result = JSONArray()
        val telephony = context.getSystemService(TelephonyManager::class.java) ?: return result
        val seen = HashSet<String>()
        val all =
            try {
                telephony.allCellInfo
            } catch (_: Exception) {
                null
            }
        for (info in all ?: emptyList()) {
            val cell = cell(info) ?: continue
            val key = "${cell.optString("radio")}/${cell.optString("area")}/${cell.optString("id")}"
            if (!seen.add(key)) continue
            result.put(cell)
            if (result.length() >= 32) break
        }
        return result
    }

    /** Cell identities are recorded as the platform reports them; no coordinate is invented. */
    private fun cell(info: CellInfo): JSONObject? {
        val identity = info.cellIdentity
        val cell = JSONObject()
        val radio: String
        val area: Long
        val id: Long
        when (identity) {
            is CellIdentityLte -> {
                radio = "lte"
                area = identity.tac.toLong()
                id = identity.ci.toLong()
                putPlmn(cell, identity.mccString, identity.mncString)
            }
            is CellIdentityNr -> {
                radio = "nr"
                area = identity.tac.toLong()
                id = identity.nci
                putPlmn(cell, identity.mccString, identity.mncString)
            }
            is CellIdentityWcdma -> {
                radio = "wcdma"
                area = identity.lac.toLong()
                id = identity.cid.toLong()
                putPlmn(cell, identity.mccString, identity.mncString)
            }
            is CellIdentityGsm -> {
                radio = "gsm"
                area = identity.lac.toLong()
                id = identity.cid.toLong()
                putPlmn(cell, identity.mccString, identity.mncString)
            }
            is CellIdentityCdma -> {
                radio = "cdma"
                area = identity.networkId.toLong()
                id = identity.basestationId.toLong()
            }
            else -> return null
        }
        cell.put("radio", radio).put("area", area).put("id", id)
        cell.put("registered", info.isRegistered)
        cell.put("dbm", info.cellSignalStrength.dbm)
        if (info.cellConnectionStatus != CellInfo.CONNECTION_NONE)
            cell.put("connection_status", info.cellConnectionStatus)
        return cell
    }

    private fun putPlmn(cell: JSONObject, mcc: String?, mnc: String?) {
        // MCC/MNC keep leading zeros as text.
        cell.put("mcc", mcc ?: "").put("mnc", mnc ?: "")
    }

    @SuppressLint("MissingPermission")
    private fun wifi(): JSONArray {
        val result = JSONArray()
        val manager = context.getSystemService(WifiManager::class.java) ?: return result
        val scanned =
            try {
                manager.scanResults
            } catch (_: Exception) {
                null
            }
        val seen = HashSet<String>()
        for (found in scanned ?: emptyList()) {
            val bssid = found.BSSID ?: continue
            if (!seen.add(bssid)) continue
            result.put(
                JSONObject()
                    .put("SSID", found.SSID ?: "")
                    .put("BSSID", bssid)
                    .put("rssi", found.level)
                    .put("frequency", found.frequency)
                    .put("scan_ms", found.timestamp / 1_000)
            )
            if (result.length() >= 128) break
        }
        return result
    }

    private fun capture(mockActive: Boolean): JSONObject =
        JSONObject()
            .put("package", context.packageName)
            .put("location_permission", permission(android.Manifest.permission.ACCESS_FINE_LOCATION))
            .put("wifi_permission", permission(wifiPermission()))
            .put("phone_permission", permission(android.Manifest.permission.READ_PHONE_STATE))
            .put(
                "location_enabled",
                context.getSystemService(LocationManager::class.java)?.isLocationEnabled == true,
            )
            .put("sdk", Build.VERSION.SDK_INT)
            .put("mock_active", mockActive)

    private fun wifiPermission(): String =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU)
            android.Manifest.permission.NEARBY_WIFI_DEVICES
        else android.Manifest.permission.ACCESS_FINE_LOCATION

    private fun permission(name: String): String =
        if (context.checkSelfPermission(name) == PackageManager.PERMISSION_GRANTED) "granted"
        else "denied"
}
