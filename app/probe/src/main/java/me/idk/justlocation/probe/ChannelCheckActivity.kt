package me.idk.justlocation.probe

import android.Manifest
import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Typeface
import android.hardware.SensorManager
import android.location.LocationManager
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import java.io.File
import java.util.concurrent.Executors

/** Displays observations and publishes the same report for device test scripts. */
class ChannelCheckActivity : Activity() {
    private lateinit var output: TextView
    private lateinit var summary: TextView
    private val handler = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor()
    private var running = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val padding = (20 * resources.displayMetrics.density).toInt()
        val content =
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(padding, padding, padding, padding)
            }
        content.addView(
            TextView(this).apply {
                text = "通道自检"
                textSize = 20f
                setPadding(0, 0, 0, 12)
            }
        )
        summary =
            TextView(this).apply {
                text = "尚未自检"
                textSize = 16f
                setPadding(0, 0, 0, 12)
            }
        content.addView(summary)
        content.addView(
            Button(this).apply {
                text = "开始自检"
                setOnClickListener { run() }
            }
        )
        output =
            TextView(this).apply {
                textSize = 12f
                typeface = Typeface.MONOSPACE
            }
        content.addView(output)
        setContentView(ScrollView(this).apply { addView(content) })
        if (intent?.getBooleanExtra(EXTRA_AUTORUN, false) == true) output.post { run() }
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == 1 && grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED)
            run()
    }

    private fun run() {
        if (running || isDestroyed) return
        val stepsOnly = intent?.getBooleanExtra("steps_only", false) == true
        val movementOnly = intent?.getBooleanExtra("movement_only", false) == true
        val continuityOnly = intent?.getBooleanExtra("continuity_only", false) == true
        val gnssOnly = intent?.getBooleanExtra("gnss_only", false) == true
        val gnssDuration = intent?.getLongExtra("gnss_duration_ms", 65000) ?: 65000
        val permission =
            if (stepsOnly) Manifest.permission.ACTIVITY_RECOGNITION
            else Manifest.permission.ACCESS_FINE_LOCATION
        if (checkSelfPermission(permission) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(permission), 1)
            return
        }
        if (continuityOnly) {
            if (
                intent.hasExtra("amap_mode") &&
                    !getSharedPreferences("amap_diagnostics", MODE_PRIVATE)
                        .getBoolean("consent", false)
            ) {
                AlertDialog.Builder(this)
                    .setTitle("高德定位 SDK 对照测试")
                    .setMessage(
                        "自检将使用高德定位 SDK，向高德发送定位及网络请求，并可能采集位置、GNSS、Wi-Fi、基站、传感器和设备信息。诊断结果保存在本机，用于比较定位来源。请阅读高德隐私政策后选择是否同意。"
                    )
                    .setNeutralButton("隐私政策") { _, _ ->
                        startActivity(
                            Intent(
                                Intent.ACTION_VIEW,
                                Uri.parse("https://lbs.amap.com/pages/privacy/"),
                            )
                        )
                    }
                    .setNegativeButton("取消", null)
                    .setPositiveButton("同意并开始") { _, _ ->
                        getSharedPreferences("amap_diagnostics", MODE_PRIVATE)
                            .edit()
                            .putBoolean("consent", true)
                            .apply()
                        run()
                    }
                    .show()
                return
            }
            startForegroundService(
                Intent(this, LocationCaptureService::class.java)
                    .putExtra("duration_ms", intent?.getLongExtra("duration_ms", 180000) ?: 180000)
                    .putExtra("amap_mode", intent.getStringExtra("amap_mode"))
                    .putExtra("amap_cache", intent.getBooleanExtra("amap_cache", false))
                    .putExtra("location_queries", intent.getBooleanExtra("location_queries", false))
            )
            summary.text = "Continuous location capture is running."
            return
        }
        running = true
        summary.text = "正在自检，约 1 分钟…"
        output.text = ""
        val context = applicationContext
        worker.execute {
            val report = CheckReport()
            try {
                report.section("app")
                report.line("package ${context.packageName}")
                report.line(
                    "location_permission ${context.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED}"
                )
                if (stepsOnly) {
                    StepChecks(context.getSystemService(SensorManager::class.java), handler)
                        .collect(report)
                } else if (gnssOnly) {
                    GnssChecks(context.getSystemService(LocationManager::class.java), handler)
                        .collect(report, gnssDuration)
                } else if (movementOnly) {
                    LocationChecks(context.getSystemService(LocationManager::class.java), handler)
                        .collectMovement(report)
                } else {
                    LocationChecks(context.getSystemService(LocationManager::class.java), handler)
                        .collect(report)
                    NetworkChecks(context).collect(report)
                }
            } catch (_: InterruptedException) {
                Thread.currentThread().interrupt()
                return@execute
            } catch (error: Throwable) {
                report.section("self-check error")
                report.line(error.toString())
            }
            val text = report.render()
            if (gnssOnly) {
                // Large raw captures can exceed logd's burst limits on some ROMs.
                val captureId = intent?.getStringExtra("capture_id") ?: "manual"
                File(filesDir, "gnss-report.txt")
                    .writeText("GNSS_CAPTURE_ID=$captureId\n$text\n$END\n")
            }
            handler.post {
                if (!isDestroyed) {
                    running = false
                    summary.text = "自检完成"
                    output.text = text
                    Log.i(TAG, BEGIN)
                    text.split('\n').forEach { Log.i(TAG, it) }
                    Log.i(TAG, END)
                }
            }
        }
    }

    override fun onDestroy() {
        worker.shutdownNow()
        super.onDestroy()
    }

    private companion object {
        const val TAG = "JustLocationCheck"
        const val BEGIN = "CHECK_REPORT_BEGIN"
        const val END = "CHECK_REPORT_END"
        const val EXTRA_AUTORUN = "autorun"
    }
}
