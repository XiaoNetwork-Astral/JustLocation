package me.idk.justlocation.probe

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
                    report.section("self-check error")
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
     *
     * <p>**收集的是一个列表而不是单个值**：`getCurrentLocation` 的回调**可能被调用多次**
     * （服务端那个注册在平台自己的投递之外，还会被我们的周期派发直接推进一次），
     * 只留最后一个值会把"中途收到过真值"这件事抹掉。2026-09-12 出现过一次 network
     * 返回真实定位，就是靠"全部值"这一列判断它是被覆盖的还是压根没被替换。
     */
    private fun requestFixes(manager: LocationManager, timeoutMs: Long = 6000): Map<String, List<android.location.Location>> {
        val result = java.util.concurrent.ConcurrentHashMap<String, java.util.concurrent.CopyOnWriteArrayList<android.location.Location>>()
        val latch = java.util.concurrent.CountDownLatch(2)
        for (provider in listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)) {
            try {
                manager.getCurrentLocation(provider, null, { runnable -> handler.post(runnable) }) { location ->
                    if (location != null) result.computeIfAbsent(provider) { java.util.concurrent.CopyOnWriteArrayList() }.add(location)
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

        // 报告的分节标题与字段名一律英文（2026-09-12 用户要求）：这些关键词是
        // test:checks 的解析依据，中文一旦在 logcat/终端链路上被编码搞坏，脚本就断在解析上。
        report.section("app")
        report.line("package $packageName")
        report.line("location_permission ${checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED}")

        report.section("location")
        // 主动要一次实时定位：`getLastKnownLocation` 读的是客户端缓存，模拟开始之前
        // 进程里可能已经存着真实位置，用它验收会得出错误结论（踩过一次）。
        val fresh = requestFixes(manager)
        for (provider in listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)) {
            val seen = fresh[provider]
            // 断言的判据仍然用**最后一个值**（应用最终看到的就是它），另外把收到的全部值附在后面。
            val fix = seen?.lastOrNull() ?: try { manager.getLastKnownLocation(provider) } catch (error: Exception) { null }
            val source = if (seen != null) "live" else "cached"
            val where = fix?.let { String.format(Locale.ROOT, "%.6f, %.6f", it.latitude, it.longitude) } ?: "none"
            val all = seen?.joinToString("|") { String.format(Locale.ROOT, "%.6f,%.6f", it.latitude, it.longitude) } ?: "none"
            report.line("$provider $where altitude ${fix?.altitude} accuracy ${fix?.accuracy} mock=${fix?.isMock} source=$source seen=${seen?.size ?: 0} all=$all")
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
        // 原始 GNSS 数据：两条出口各有自己的回调，与卫星状态不是同一条接口。
        val measurementCount = java.util.concurrent.atomic.AtomicInteger()
        val measurementSats = java.util.concurrent.atomic.AtomicInteger(-1)
        val measurementClock = java.util.concurrent.atomic.AtomicReference<String>()
        val measurementCallback = object : android.location.GnssMeasurementsEvent.Callback() {
            override fun onGnssMeasurementsReceived(event: android.location.GnssMeasurementsEvent) {
                measurementCount.incrementAndGet()
                measurementSats.set(event.measurements.size)
                val clock = event.clock
                measurementClock.set("timeNanos=${clock.timeNanos} fullBiasNanos=${clock.fullBiasNanos}")
            }
        }
        val messageCount = java.util.concurrent.atomic.AtomicInteger()
        val messageStatus = java.util.concurrent.atomic.AtomicInteger(Int.MIN_VALUE)
        val messageDetail = java.util.concurrent.atomic.AtomicReference<String>()
        val messageCallback = object : android.location.GnssNavigationMessage.Callback() {
            override fun onGnssNavigationMessageReceived(message: android.location.GnssNavigationMessage) {
                messageCount.incrementAndGet()
                if (messageDetail.get() == null) {
                    messageDetail.set("type=${message.type} svid=${message.svid} subframe=${message.submessageId} " +
                            "status=${message.status} length=${message.data?.size}")
                }
            }
            override fun onStatusChanged(status: Int) { messageStatus.set(status) }
        }
        var measurementRegistered = false
        var messageRegistered = false
        try {
            manager.registerGnssMeasurementsCallback({ runnable -> handler.post(runnable) }, measurementCallback)
            measurementRegistered = true
        } catch (error: Exception) { report.line("measurements registration failed ${error.message}") }
        try {
            manager.registerGnssNavigationMessageCallback({ runnable -> handler.post(runnable) }, messageCallback)
            messageRegistered = true
        } catch (error: Exception) { report.line("navigation message registration failed ${error.message}") }
        // 卫星状态与 NMEA 都由系统的 GPS 会话驱动；先注册回调，再另发一次位置请求把会话拉起来。
        try { manager.registerGnssStatusCallback(status, handler) } catch (error: Exception) { report.line("status callback registration failed ${error.message}") }
        try { manager.addNmeaListener(nmeaListener, handler) } catch (error: Exception) { report.line("NMEA registration failed ${error.message}") }
        try { manager.getCurrentLocation(LocationManager.GPS_PROVIDER, null, { runnable -> handler.post(runnable) }, { }) }
        catch (error: Exception) { report.line("single GPS request failed ${error.message}") }
        Thread.sleep(5000)
        try { manager.removeNmeaListener(nmeaListener) } catch (_: Exception) { }
        try { manager.unregisterGnssStatusCallback(status) } catch (_: Exception) { }
        if (measurementRegistered) try { manager.unregisterGnssMeasurementsCallback(measurementCallback) } catch (_: Exception) { }
        if (messageRegistered) try { manager.unregisterGnssNavigationMessageCallback(messageCallback) } catch (_: Exception) { }

        report.section("raw gnss")
        report.line("measurements registered=$measurementRegistered seen=${measurementCount.get()} satellites=${measurementSats.get()} " +
                "clock=${measurementClock.get() ?: "none"}")
        report.line("navigation registered=$messageRegistered seen=${messageCount.get()} " +
                "capability=${if (messageStatus.get() == Int.MIN_VALUE) "no callback" else messageStatus.get().toString()} " +
                "first=${messageDetail.get() ?: "none"}")
        report.line(parcelRoundTrip())

        report.section("continuous subscription")
        report.line(continuousUpdates(manager))

        report.section("satellites")
        synchronized(satellites) {
            if (satellites.isEmpty()) report.line("no satellite status callback")
            else {
                report.line("count ${satellites.size}")
                report.line("svid/cn0 ${satellites.joinToString(" ")}")
            }
        }

        report.section("nmea")
        synchronized(nmea) {
            report.line("sentences ${nmea.size}")
            report.line("types ${nmea.map { it.substringAfter('$').take(5) }.distinct().joinToString(" ")}")
            nmea.take(2).forEach { report.line(it) }
        }

        report.section("cells")
        val cells: List<CellInfo>? = try { telephony.allCellInfo } catch (error: Exception) { null }
        if (cells == null) report.line("read failed (permission missing?)")
        else if (cells.isEmpty()) report.line("no cell info")
        else for (cell in cells.take(6)) {
            report.line(describe(cell))
        }

        report.section("sim")
        report.line("network_operator ${telephony.networkOperatorName} / ${telephony.networkOperator}")
        report.line("sim_operator ${telephony.simOperatorName} / ${telephony.simOperator}")
        report.line("sim_state ${telephony.simState}")
        // 订阅信息在 SubscriptionManager 上，不在 TelephonyManager 上。
        val subscriptions = getSystemService(SubscriptionManager::class.java)
        val list = try { subscriptions?.activeSubscriptionInfoList } catch (error: Exception) { null }
        if (list == null) report.line("subscription list unreadable (needs READ_PHONE_STATE or more permission)")
        else for (info in list) {
            report.line("slot ${info.simSlotIndex} subscription ${info.subscriptionId} ${info.carrierName} " +
                    "${info.mccString}${info.mncString} ${info.countryIso}")
        }

        report.section("wifi")
        val wifi = getSystemService(android.net.wifi.WifiManager::class.java)
        val info = try { wifi.connectionInfo } catch (error: Exception) { null }
        report.line(if (info == null) "read failed"
        else "SSID ${info.ssid} BSSID ${info.bssid} rssi ${info.rssi} " +
                "state ${info.supplicantState} network ${info.networkId}")
        // 附近网络走的是另一条服务端出口（`getScanResults`），所以单独读、单独报。
        // 缺定位权限时系统会抛 SecurityException——如实报出来，不要吞掉当成"没有附近网络"。
        val nearby = try { wifi.scanResults } catch (error: Exception) { null }
        report.line(when {
            nearby == null -> "nearby networks read failed"
            else -> "nearby networks ${nearby.size}:" + nearby.joinToString(" / ") { "${it.SSID}(${it.BSSID} ${it.level})" }
        })
        // 第三条取数路径，**目前不归我们接管**：应用也可以从 ConnectivityManager 的
        // `NetworkCapabilities.transportInfo` 拿 WifiInfo。它由 Wi-Fi 服务自己的 NetworkAgent 填充，
        // 和上面两个出口不是一回事。这里如实读出来，用来把"覆盖到哪"钉在报告里；
        // 断言只在脚本里做（见 run-checks.mjs），读不到就记 KNOWN-GAP，不算失败。
        val connectivity = getSystemService(android.net.ConnectivityManager::class.java)
        val active = connectivity?.activeNetwork
        val caps = if (active == null) null else connectivity?.getNetworkCapabilities(active)
        val transport = caps?.transportInfo
        report.line(if (transport is android.net.wifi.WifiInfo)
            "NetworkCapabilities ${transport.ssid} ${transport.bssid} rssi ${transport.rssi}"
        else "NetworkCapabilities has no WifiInfo (${transport?.javaClass?.simpleName ?: "no active network"})")
    }

    /**
     * `GnssNavigationMessage` 在本进程里的 Parcel 往返。
     *
     * <p>为什么要做这个检查：真机上导航电文送达时报的是
     * `IllegalArgumentException: Expected receiver of type GnssMeasurement but got GnssNavigationMessage`，
     * 而这句话是**反序列化时**抛的（`Parcel.readTypedObject` 的类型核对）。也就是说问题可能不在
     * 谁把消息发给了谁，而在**这个类在应用进程里能不能被正确还原**。往返一次就能判定：
     * 还原成功 = 类本身没问题；抛异常或还原成别的类型 = 问题在类与 ROM 这一侧，与模块无关。
     */
    private fun parcelRoundTrip(): String {
        // SDK 的 android.jar 桩里这个构造器是包内可见的（真机上才公开），所以只能反射取——
        // 模块侧 `GnssRawOutput` 走的是同一条路。
        val messageType = android.location.GnssNavigationMessage::class.java
        return try {
            val constructor = messageType.getDeclaredConstructor()
            constructor.isAccessible = true
            val message = constructor.newInstance()
            val type = messageType.getDeclaredField("mType")
            type.isAccessible = true
            type.setInt(message, 257)
            val parcel = android.os.Parcel.obtain()
            try {
                parcel.writeParcelable(message, 0)
                parcel.setDataPosition(0)
                val restored = parcel.readParcelable<android.os.Parcelable>(messageType.classLoader)
                "parcel_roundtrip written=${message.javaClass.name} read=${restored?.javaClass?.name ?: "null"} " +
                        "same_type=${restored is android.location.GnssNavigationMessage}"
            } finally { parcel.recycle() }
        } catch (error: Throwable) {
            val cause = generateSequence(error) { it.cause }.last()
            "parcel_roundtrip failed=${cause.javaClass.name}: ${cause.message}"
        }
    }

    /**
     * 像地图软件那样**持续订阅**一段时间的定位，看晚些时候会不会混进真实坐标。
     *
     * <p>为什么要单独做这一条：`getCurrentLocation` 是一次性请求、窗口只有几秒，
     * 而地图软件是长时间挂着订阅。真机现象是高德刚进去显示模拟位置、过一会儿回到真实位置，
     * 所以"短窗口正常"不能证明"长订阅正常"——必须按地图软件的用法测一遍。
     */
    private fun continuousUpdates(manager: LocationManager): String {
        val seen = java.util.concurrent.CopyOnWriteArrayList<String>()
        val listener = android.location.LocationListener { location ->
            if (location != null && seen.size < 400) {
                seen.add(String.format(Locale.ROOT, "%.6f,%.6f", location.latitude, location.longitude))
            }
        }
        var requested = 0
        for (provider in listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)) {
            try {
                manager.requestLocationUpdates(provider, 1000L, 0f, listener, handler.looper)
                requested++
            } catch (error: Exception) { /* 下面按收到的条数报，不吞掉结论 */ }
        }
        Thread.sleep(60_000)
        try { manager.removeUpdates(listener) } catch (_: Exception) { }
        val distinct = seen.distinct()
        return "requested=$requested seen=${seen.size} distinct=${distinct.size} " +
                "first=${seen.firstOrNull() ?: "none"} last=${seen.lastOrNull() ?: "none"} " +
                "all_distinct=${distinct.joinToString("|").ifEmpty { "none" }}"
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
