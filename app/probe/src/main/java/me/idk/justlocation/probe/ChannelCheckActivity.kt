package me.idk.justlocation.probe

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.graphics.Typeface
import android.location.LocationManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
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
        if (
            checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) !=
                PackageManager.PERMISSION_GRANTED
        ) {
            requestPermissions(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION), 1)
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
                LocationChecks(context.getSystemService(LocationManager::class.java), handler)
                    .collect(report)
                NetworkChecks(context).collect(report)
            } catch (_: InterruptedException) {
                Thread.currentThread().interrupt()
                return@execute
            } catch (error: Throwable) {
                report.section("self-check error")
                report.line(error.toString())
            }
            val text = report.render()
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
