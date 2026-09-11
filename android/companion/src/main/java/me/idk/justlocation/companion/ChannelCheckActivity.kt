package me.idk.justlocation.companion

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.location.GnssStatus
import android.location.LocationManager
import android.location.OnNmeaMessageListener
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.telephony.CellInfo
import android.telephony.SubscriptionManager
import android.telephony.TelephonyManager
import android.util.Log
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import java.util.Locale

/**
 * 逐通道自检：把每条通道**从普通应用能看到的样子**读出来。
 *
 * <p>存在的理由：定位之外的通道没有现成的界面入口，而"模块装上了"不等于"输出被替换了"。
 * 这里只调用公开接口读取，报告的是事实；判定由 `tests/device/run-checks.mjs` 按同一份
 * 文本做，避免判定规则散落在两处。
 *
 * <p>输出分两路：界面上的可滚动报告，以及 logcat 里带固定标记的同一份文本
 * （tag `JustLocationCheck`，`CHECK_REPORT_BEGIN` 与 `CHECK_REPORT_END` 之间），后者供脚本抓取。
 */
class ChannelCheckActivity : Activity() {
    private lateinit var output: TextView
    private lateinit var summary: TextView
    private val handler = Handler(Looper.getMainLooper())

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val padding = (20 * resources.displayMetrics.density).toInt()
        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(padding, padding, padding, padding)
        }
        content.addView(TextView(this).apply { text = "通道自检"; textSize = 20f; setPadding(0, 0, 0, 12) })
        summary = TextView(this).apply { text = "尚未自检"; textSize = 16f; setPadding(0, 0, 0, 12) }
        content.addView(summary)
        content.addView(Button(this).apply { text = "开始自检"; setOnClickListener { run() } })
        output = TextView(this).apply { textSize = 12f; typeface = android.graphics.Typeface.MONOSPACE }
        content.addView(output)
        setContentView(ScrollView(this).apply { addView(content) })
        // 由测试脚本经 adb 启动时自动开跑，免得再去点按钮；从主页面进来则不动，
        // 因为那时通常还没授权，会立刻弹出权限对话框。
        if (intent?.getBooleanExtra(EXTRA_AUTORUN, false) == true) output.post { run() }
    }

    private fun permitted(action: () -> Unit) {
        if (checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED) action()
        else requestPermissions(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION), 1)
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == 1 && grantResults.isNotEmpty() && grantResults[0] == PackageManager.PERMISSION_GRANTED) run()
    }

    private fun run() {
        permitted {
            summary.text = "正在自检，约 6 秒…"
            output.text = ""
            val report = Report()
            // 采集要等回调陆续到达，所以放在后台线程；主线程保持空闲，否则界面无法被 dump。
            val worker = Thread({
                try {
                    collect(report)
                } catch (error: Throwable) {
                    report.section("自检异常")
                    report.line(error.toString())
                }
                val text = report.render()
                handler.post {
                    summary.text = "自检完成"
                    output.text = text
                    Log.i(TAG, BEGIN)
                    text.split('\n').forEach { Log.i(TAG, it) }
                    Log.i(TAG, END)
                }
            }, "JustLocation-check")
            worker.start()
        }
    }

    /**
     * 向两个 provider 各要一次实时定位，最多等 [timeoutMs]。
     *
     * <p>用主线程的 `Handler` 做执行器；等待发生在后台线程上，界面不卡。
     */
    private fun requestFixes(manager: LocationManager, timeoutMs: Long = 6000): Map<String, android.location.Location> {
        val result = java.util.concurrent.ConcurrentHashMap<String, android.location.Location>()
        val latch = java.util.concurrent.CountDownLatch(2)
        for (provider in listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)) {
            try {
                manager.getCurrentLocation(provider, null, { runnable -> handler.post(runnable) }) { location ->
                    if (location != null) result[provider] = location
                    latch.countDown()
                }
            } catch (error: Exception) {
                latch.countDown()
            }
        }
        latch.await(timeoutMs, java.util.concurrent.TimeUnit.MILLISECONDS)
        return result
    }

    private fun collect(report: Report) {
        val manager = getSystemService(LocationManager::class.java)
        val telephony = getSystemService(TelephonyManager::class.java)

        report.section("应用")
        report.line("包名 $packageName")
        report.line("定位权限 ${checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED}")

        report.section("定位")
        // 主动要一次实时定位：`getLastKnownLocation` 读的是客户端缓存，模拟开始之前
        // 进程里可能已经存着真实位置，用它验收会得出错误结论（踩过一次）。
        val fresh = requestFixes(manager)
        for (provider in listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)) {
            val fix = fresh[provider] ?: try { manager.getLastKnownLocation(provider) } catch (error: Exception) { null }
            val source = if (fresh.containsKey(provider)) "实时" else "缓存"
            val where = fix?.let { String.format(Locale.ROOT, "%.6f, %.6f", it.latitude, it.longitude) } ?: "无"
            report.line("$provider $where 海拔 ${fix?.altitude} 精度 ${fix?.accuracy} mock=${fix?.isMock} 来源=$source")
        }

        val satellites = ArrayList<String>()
        val nmea = ArrayList<String>()
        val status = object : GnssStatus.Callback() {
            override fun onSatelliteStatusChanged(value: GnssStatus) {
                synchronized(satellites) {
                    satellites.clear()
                    for (index in 0 until value.satelliteCount) {
                        satellites.add("${value.getSvid(index)}/${value.getCn0DbHz(index).toInt()}")
                    }
                }
            }
        }
        val nmeaListener = OnNmeaMessageListener { message, _ ->
            synchronized(nmea) { if (nmea.size < 200) nmea.add(message.trim()) }
        }
        // 卫星状态与 NMEA 都由系统的 GPS 会话驱动；先注册回调，再另发一次位置请求把会话拉起来。
        try { manager.registerGnssStatusCallback(status, handler) } catch (error: Exception) { report.line("卫星回调注册失败 ${error.message}") }
        try { manager.addNmeaListener(nmeaListener, handler) } catch (error: Exception) { report.line("NMEA 注册失败 ${error.message}") }
        try { manager.getCurrentLocation(LocationManager.GPS_PROVIDER, null, { runnable -> handler.post(runnable) }, { }) }
        catch (error: Exception) { report.line("GPS 单次请求失败 ${error.message}") }
        Thread.sleep(5000)
        try { manager.removeNmeaListener(nmeaListener) } catch (_: Exception) { }
        try { manager.unregisterGnssStatusCallback(status) } catch (_: Exception) { }

        report.section("卫星状态")
        synchronized(satellites) {
            if (satellites.isEmpty()) report.line("没有卫星状态回调")
            else {
                report.line("卫星数 ${satellites.size}")
                report.line("星号/信噪 ${satellites.joinToString(" ")}")
            }
        }

        report.section("NMEA")
        synchronized(nmea) {
            report.line("报文数 ${nmea.size}")
            report.line("语句类型 ${nmea.map { it.substringAfter('$').take(5) }.distinct().joinToString(" ")}")
            nmea.take(2).forEach { report.line(it) }
        }

        report.section("基站")
        val cells: List<CellInfo>? = try { telephony.allCellInfo } catch (error: Exception) { null }
        if (cells == null) report.line("读取失败（可能缺少权限）")
        else if (cells.isEmpty()) report.line("没有基站信息")
        else for (cell in cells.take(6)) {
            report.line(describe(cell))
        }

        report.section("SIM")
        report.line("网络运营商 ${telephony.networkOperatorName} / ${telephony.networkOperator}")
        report.line("SIM 运营商 ${telephony.simOperatorName} / ${telephony.simOperator}")
        report.line("卡状态 ${telephony.simState}")
        // 订阅信息在 SubscriptionManager 上，不在 TelephonyManager 上。
        val subscriptions = getSystemService(SubscriptionManager::class.java)
        val list = try { subscriptions?.activeSubscriptionInfoList } catch (error: Exception) { null }
        if (list == null) report.line("订阅列表不可读（需要 READ_PHONE_STATE 或权限不足）")
        else for (info in list) {
            report.line("卡槽 ${info.simSlotIndex} 订阅 ${info.subscriptionId} ${info.carrierName} " +
                    "${info.mccString}${info.mncString} ${info.countryIso}")
        }

        report.section("Wi-Fi")
        val wifi = getSystemService(android.net.wifi.WifiManager::class.java)
        val info = try { wifi.connectionInfo } catch (error: Exception) { null }
        report.line(if (info == null) "读取失败" else "SSID ${info.ssid} BSSID ${info.bssid} 信号 ${info.rssi}")
    }

    /**
     * 基站身份的可辨别字段。
     *
     * <p>`CellIdentity` 各代的公开取值方法有增删（`CellInfoNr` 上就没有 `tac`），逐个点名会随
     * SDK 变化编译不过；这里按字段名反射取值，取不到就跳过。**只读**，不写任何字段。
     * 需要这些字段是因为真实基站与模拟基站可能同属一个 PLMN，光看 MCC/MNC 分辨不出来。
     */
    private fun describe(cell: CellInfo): String {
        val identity = cell.cellIdentity
        val fields = listOf("mMcc", "mMnc", "mTac", "mCi", "mNci", "mLac", "mCid", "mPci")
        val values = fields.mapNotNull { name ->
            val value = try {
                identity.javaClass.getMethod("get" + name.removePrefix("m")).invoke(identity)
            } catch (_: Exception) {
                try { identity.javaClass.getDeclaredField(name).apply { isAccessible = true }.get(identity) }
                catch (_: Exception) { null }
            }
            value?.let { "${name.removePrefix("m")}=$it" }
        }
        return "${cell.javaClass.simpleName} registered=${cell.isRegistered} ${values.joinToString(" ")}"
    }

    /** 只累积"读到了什么"；判定在设备测试脚本里。 */
    private class Report {
        private val lines = ArrayList<String>()
        fun section(title: String) { lines.add(""); lines.add("## $title") }
        fun line(value: String) { lines.add(value) }
        fun render(): String = lines.joinToString("\n")
    }

    private companion object {
        const val TAG = "JustLocationCheck"
        const val BEGIN = "CHECK_REPORT_BEGIN"
        const val END = "CHECK_REPORT_END"

        /** 置 true 时进入页面即开始自检；设备测试脚本用它。 */
        const val EXTRA_AUTORUN = "autorun"
    }
}
