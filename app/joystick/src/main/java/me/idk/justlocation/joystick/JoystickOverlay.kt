package me.idk.justlocation.joystick

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewOutlineProvider
import android.view.WindowManager
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.Switch
import android.widget.TextView
import kotlin.math.ln
import kotlin.math.pow
import kotlin.math.round

/** The disc remains visible; settings and library pages share a bounded floating card. */
internal class JoystickOverlay(
    private val context: Context,
    private val changed: (StickInput) -> Unit,
    private val release: () -> Unit,
    private val action: (Action) -> Unit,
    private val speedChanged: (Double) -> Unit,
) {
    enum class Action {
        LOCK,
        FOLLOW,
        PLACES,
        ROUTES,
        SAVE,
        PAUSE,
        RESUME,
        STOP,
        START,
        STEPS,
        GNSS,
        CLOSE,
    }

    private enum class Page {
        SETTINGS,
        PLACES,
        ROUTES,
        NAME,
    }

    data class Entry(val id: String, val name: String, val detail: String)

    private val windows = context.getSystemService(WindowManager::class.java)
    private val teal = Color.rgb(0, 137, 123)
    private val ink = Color.rgb(84, 103, 106)
    private val preferences = context.getSharedPreferences("overlay", Context.MODE_PRIVATE)
    private var panel: LinearLayout? = null
    private lateinit var params: WindowManager.LayoutParams
    private lateinit var stick: StickView
    private lateinit var toggle: OverlayIcon
    private lateinit var card: FrameLayout
    private lateinit var settings: LinearLayout
    private lateinit var settingsScroll: ScrollView
    private lateinit var subpage: LinearLayout
    private lateinit var direction: TextView
    private lateinit var speedText: TextView
    private lateinit var slider: SeekBar
    private lateinit var lock: Switch
    private lateinit var steps: Switch
    private lateinit var gnss: Switch
    private lateinit var simulation: Switch
    private lateinit var details: TextView
    private lateinit var routeActions: LinearLayout
    private lateinit var pause: TextView
    private var expanded = false
    private var page = Page.SETTINGS
    private var active = false
    private var paused = false
    private var sliderTouch = false
    private var speed = 1.5
    private var listBody: LinearLayout? = null

    private fun dp(value: Int) = (value * context.resources.displayMetrics.density).toInt()

    private fun s(id: Int) = context.getString(id)

    private fun column() = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }

    private fun background(color: Int, radius: Int) =
        GradientDrawable().apply {
            setColor(color)
            cornerRadius = dp(radius).toFloat()
        }

    private fun label(text: String, size: Float = 13f, color: Int = ink) =
        TextView(context).apply {
            this.text = text
            textSize = size
            setTextColor(color)
            gravity = Gravity.CENTER_VERTICAL
        }

    private fun clickable(view: View, clicked: () -> Unit) {
        val attrs =
            context.obtainStyledAttributes(intArrayOf(android.R.attr.selectableItemBackground))
        view.foreground = attrs.getDrawable(0)
        attrs.recycle()
        view.isFocusable = true
        view.setOnClickListener { clicked() }
    }

    private fun divider(parent: LinearLayout) {
        parent.addView(
            View(context).apply { setBackgroundColor(Color.rgb(222, 225, 225)) },
            LinearLayout.LayoutParams(-1, dp(1)),
        )
    }

    private fun actionRow(
        parent: LinearLayout,
        title: Int,
        operation: Action,
        arrow: Boolean = true,
    ) {
        val row = LinearLayout(context).apply { gravity = Gravity.CENTER_VERTICAL }
        row.addView(label(s(title)), LinearLayout.LayoutParams(0, dp(36), 1f))
        if (arrow) row.addView(label("›", 23f, teal), LinearLayout.LayoutParams(dp(18), dp(36)))
        clickable(row) { action(operation) }
        parent.addView(row)
        divider(parent)
    }

    @Suppress("DEPRECATION")
    private fun switchRow(parent: LinearLayout, title: Int, operation: Action): Switch {
        val control =
            Switch(context).apply {
                text = s(title)
                textSize = 13f
                setTextColor(ink)
                showText = false
                thumbTintList =
                    ColorStateList(
                        arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf()),
                        intArrayOf(teal, Color.rgb(236, 236, 236)),
                    )
                trackTintList =
                    ColorStateList(
                        arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf()),
                        intArrayOf(Color.rgb(159, 209, 203), Color.rgb(192, 196, 196)),
                    )
                setPadding(0, 0, dp(4), 0)
                setOnClickListener {
                    isChecked = !isChecked // Render only acknowledged backend/controller state.
                    action(operation)
                }
            }
        parent.addView(control, LinearLayout.LayoutParams(-1, dp(36)))
        divider(parent)
        return control
    }

    fun show(initialSpeed: Double) {
        speed = initialSpeed
        val bounds = windows.currentWindowMetrics.bounds
        params =
            WindowManager.LayoutParams(
                    dp(132),
                    -2,
                    WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                    WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE,
                    PixelFormat.TRANSLUCENT,
                )
                .apply {
                    gravity = Gravity.TOP or Gravity.START
                    x = preferences.getInt("x", bounds.width() / 2 - dp(66))
                    y = preferences.getInt("y", bounds.height() / 2)
                    softInputMode = WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE
                }
        val root =
            column().apply {
                setPadding(dp(6), dp(4), dp(6), dp(8))
                clipChildren = false
                clipToPadding = false
            }
        panel = root
        stick = StickView(context, changed, release)
        root.addView(stick, LinearLayout.LayoutParams(dp(120), dp(120)))
        val toolbar =
            LinearLayout(context).apply {
                gravity = Gravity.CENTER
                background = background(Color.WHITE, 20)
                outlineProvider = ViewOutlineProvider.BACKGROUND
                clipToOutline = true
                elevation = dp(2).toFloat()
            }
        val handle =
            OverlayIcon(context, OverlayIcon.Symbol.MENU, Color.GRAY).apply {
                contentDescription = s(R.string.joystick_drag)
            }
        toolbar.addView(handle, LinearLayout.LayoutParams(dp(48), dp(28)))
        toggle = OverlayIcon(context, OverlayIcon.Symbol.EXPAND, teal)
        toolbar.addView(toggle, LinearLayout.LayoutParams(dp(32), dp(28)))
        clickable(toggle) { toggleExpanded() }
        installDrag(handle)
        root.addView(
            toolbar,
            LinearLayout.LayoutParams(dp(80), dp(28)).apply {
                leftMargin = dp(20)
                topMargin = dp(4)
                bottomMargin = dp(10)
            },
        )
        card =
            FrameLayout(context).apply {
                background = background(Color.WHITE, 5)
                outlineProvider = ViewOutlineProvider.BACKGROUND
                clipToOutline = true
                elevation = dp(3).toFloat()
                visibility = View.GONE
            }
        settings = buildSettings()
        subpage = column().apply { visibility = View.GONE }
        card.addView(settings, FrameLayout.LayoutParams(-1, -1))
        card.addView(subpage, FrameLayout.LayoutParams(-1, -1))
        root.addView(card, LinearLayout.LayoutParams(dp(220), dp(240)))
        toggle.contentDescription = s(R.string.joystick_expand)
        updateSpeed()
        windows.addView(root, params)
        clamp()
    }

    private fun buildSettings(): LinearLayout {
        val result = column().apply { setPadding(dp(14), 0, dp(14), 0) }
        val scroll =
            ScrollView(context).apply {
                isFillViewport = false
                isVerticalScrollBarEnabled = true
            }
        settingsScroll = scroll
        val body = column()
        val directionRow = LinearLayout(context).apply { gravity = Gravity.CENTER_VERTICAL }
        directionRow.addView(
            label(s(R.string.joystick_direction)),
            LinearLayout.LayoutParams(0, dp(40), 1f),
        )
        direction =
            label(s(R.string.direction_north), 12f, teal).apply {
                gravity = Gravity.CENTER_VERTICAL or Gravity.END
            }
        directionRow.addView(direction, LinearLayout.LayoutParams(dp(100), dp(40)))
        clickable(directionRow) { action(Action.FOLLOW) }
        body.addView(directionRow)
        divider(body)
        val speedRow = LinearLayout(context)
        speedRow.addView(
            label(s(R.string.joystick_speed)),
            LinearLayout.LayoutParams(0, dp(32), 1f),
        )
        speedText = label("", 13f, teal)
        speedRow.addView(speedText, LinearLayout.LayoutParams(-2, dp(32)))
        body.addView(speedRow)
        slider =
            SeekBar(context).apply {
                max = 1000
                progressTintList = ColorStateList.valueOf(teal)
                thumbTintList = ColorStateList.valueOf(teal)
                setPadding(dp(8), 0, dp(8), 0)
                contentDescription = s(R.string.joystick_speed)
                setOnSeekBarChangeListener(
                    object : SeekBar.OnSeekBarChangeListener {
                        override fun onStartTrackingTouch(view: SeekBar) {
                            sliderTouch = true
                        }

                        override fun onStopTrackingTouch(view: SeekBar) {
                            sliderTouch = false
                            updateSpeed()
                        }

                        override fun onProgressChanged(
                            view: SeekBar,
                            progress: Int,
                            fromUser: Boolean,
                        ) {
                            if (fromUser)
                                speedChanged(
                                    (round(10000.0.pow(progress / 1000.0)) / 10.0).coerceIn(
                                        0.1,
                                        1000.0,
                                    )
                                )
                        }
                    }
                )
            }
        body.addView(slider, LinearLayout.LayoutParams(-1, dp(20)))
        val presets = LinearLayout(context)
        val symbols =
            listOf(
                OverlayIcon.Symbol.WALK,
                OverlayIcon.Symbol.RUN,
                OverlayIcon.Symbol.BIKE,
                OverlayIcon.Symbol.CAR,
                OverlayIcon.Symbol.PLANE,
            )
        val titles =
            listOf(
                R.string.speed_walk,
                R.string.speed_run,
                R.string.speed_cycle,
                R.string.speed_drive,
                R.string.speed_fly,
            )
        val speeds = listOf(1.0, 3.0, 6.0, 15.0, 60.0)
        symbols.forEachIndexed { index, symbol ->
            val icon =
                OverlayIcon(context, symbol, teal).apply {
                    contentDescription =
                        s(titles[index]) +
                            " " +
                            context.getString(R.string.speed_value, speeds[index])
                }
            clickable(icon) { speedChanged(speeds[index]) }
            presets.addView(icon, LinearLayout.LayoutParams(0, dp(28), 1f))
        }
        body.addView(presets)
        divider(body)
        lock = switchRow(body, R.string.direction_lock, Action.LOCK)
        steps = switchRow(body, R.string.steps_toggle, Action.STEPS)
        gnss = switchRow(body, R.string.gnss_toggle, Action.GNSS)
        actionRow(body, R.string.places_title, Action.PLACES)
        actionRow(body, R.string.save_position, Action.SAVE, false)
        actionRow(body, R.string.routes_title, Action.ROUTES)
        details = label("", 11f).apply { setPadding(0, dp(6), 0, dp(6)) }
        body.addView(details)
        routeActions = LinearLayout(context).apply { visibility = View.GONE }
        pause = label(s(R.string.pause_route), 12f, teal).apply { gravity = Gravity.CENTER }
        clickable(pause) { action(if (paused) Action.RESUME else Action.PAUSE) }
        routeActions.addView(pause, LinearLayout.LayoutParams(0, dp(38), 1f))
        val stop = label(s(R.string.stop_route), 12f, teal).apply { gravity = Gravity.CENTER }
        clickable(stop) { action(Action.STOP) }
        routeActions.addView(stop, LinearLayout.LayoutParams(0, dp(38), 1f))
        body.addView(routeActions)
        scroll.addView(body)
        result.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        divider(result)
        val footer = LinearLayout(context).apply { gravity = Gravity.CENTER_VERTICAL }
        simulation =
            Switch(context).apply {
                text = s(R.string.simulation)
                textSize = 13f
                setTextColor(ink)
                thumbTintList = lock.thumbTintList
                trackTintList = lock.trackTintList
                setPadding(0, 0, dp(10), 0)
                setOnClickListener {
                    isChecked = active
                    action(if (active) Action.STOP else Action.START)
                }
            }
        footer.addView(simulation, LinearLayout.LayoutParams(0, dp(44), 1f))
        footer.addView(
            View(context).apply { setBackgroundColor(Color.rgb(222, 225, 225)) },
            LinearLayout.LayoutParams(dp(1), dp(24)),
        )
        val close = label(s(R.string.close_joystick), 13f).apply { gravity = Gravity.CENTER }
        clickable(close) { action(Action.CLOSE) }
        footer.addView(close, LinearLayout.LayoutParams(0, dp(44), 1f))
        result.addView(footer)
        return result
    }

    fun render(
        mode: MotionController.Mode,
        motion: StickInput,
        speed: Double,
        active: Boolean,
        stepsEnabled: Boolean,
        gnssEnabled: Boolean,
        route: Boolean,
        paused: Boolean,
        completed: Boolean,
        status: String,
    ) {
        this.speed = speed
        this.active = active
        this.paused = paused
        updateSpeed()
        direction.text =
            s(
                if (mode == MotionController.Mode.FOLLOW) R.string.direction_follow
                else R.string.direction_north
            )
        lock.isChecked = mode == MotionController.Mode.LOCKED
        steps.isChecked = stepsEnabled
        gnss.isChecked = gnssEnabled
        simulation.isChecked = active
        details.text = status
        details.visibility = if (status.isEmpty()) View.GONE else View.VISIBLE
        routeActions.visibility = if (route) View.VISIBLE else View.GONE
        pause.text = s(if (paused) R.string.resume_route else R.string.pause_route)
        pause.isEnabled = !completed
        pause.alpha = if (completed) 0.4f else 1f
        stick.showMotion(motion)
    }

    private fun updateSpeed() {
        speedText.text = context.getString(R.string.speed_value, speed)
        if (!sliderTouch)
            slider.progress = (ln(speed.coerceIn(0.1, 1000.0) / 0.1) / ln(10000.0) * 1000).toInt()
    }

    private fun toggleExpanded() {
        expanded = !expanded
        if (!expanded) showSettings()
        card.visibility = if (expanded) View.VISIBLE else View.GONE
        toggle.symbol = if (expanded) OverlayIcon.Symbol.COLLAPSE else OverlayIcon.Symbol.EXPAND
        toggle.contentDescription =
            s(if (expanded) R.string.joystick_collapse else R.string.joystick_expand)
        params.width = dp(if (expanded) 232 else 132)
        clamp()
        if (expanded) settingsScroll.post { settingsScroll.scrollTo(0, 0) }
    }

    private fun installDrag(handle: View) {
        var x = 0
        var y = 0
        var rawX = 0f
        var rawY = 0f
        var dragging = false
        val slop = ViewConfiguration.get(context).scaledTouchSlop
        handle.setOnTouchListener { _, event ->
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    x = params.x
                    y = params.y
                    rawX = event.rawX
                    rawY = event.rawY
                    dragging = false
                }
                MotionEvent.ACTION_MOVE -> {
                    if (
                        !dragging &&
                            (kotlin.math.abs(event.rawX - rawX) > slop ||
                                kotlin.math.abs(event.rawY - rawY) > slop)
                    ) {
                        dragging = true
                        release()
                    }
                    if (dragging) {
                        params.x = x + (event.rawX - rawX).toInt()
                        params.y = y + (event.rawY - rawY).toInt()
                        clamp()
                    }
                }
                MotionEvent.ACTION_UP ->
                    if (dragging)
                        preferences.edit().putInt("x", params.x).putInt("y", params.y).apply()
                    else handle.performClick()
                MotionEvent.ACTION_CANCEL -> if (dragging) release()
            }
            true
        }
        handle.setOnClickListener { toggleExpanded() }
        handle.isFocusable = true
    }

    private fun clamp() {
        val root = panel ?: return
        val bounds = windows.currentWindowMetrics.bounds
        val height = dp(if (expanded) 418 else 170)
        params.x = params.x.coerceIn(0, (bounds.width() - params.width).coerceAtLeast(0))
        params.y = params.y.coerceIn(0, (bounds.height() - height - dp(32)).coerceAtLeast(0))
        windows.updateViewLayout(root, params)
    }

    fun configurationChanged() {
        if (panel != null) clamp()
    }

    fun showSettings() {
        if (page == Page.NAME) setFocusable(false)
        page = Page.SETTINGS
        listBody = null
        settings.visibility = View.VISIBLE
        subpage.visibility = View.GONE
        subpage.removeAllViews()
    }

    private fun pageHeader(title: Int, add: Boolean = false) {
        subpage.removeAllViews()
        settings.visibility = View.GONE
        subpage.visibility = View.VISIBLE
        val row = LinearLayout(context).apply { gravity = Gravity.CENTER_VERTICAL }
        val back =
            OverlayIcon(context, OverlayIcon.Symbol.BACK, teal).apply {
                contentDescription = s(R.string.back)
            }
        clickable(back) { showSettings() }
        row.addView(back, LinearLayout.LayoutParams(dp(40), dp(40)))
        row.addView(label(s(title), 14f, teal), LinearLayout.LayoutParams(0, dp(40), 1f))
        if (add) {
            val plus =
                OverlayIcon(context, OverlayIcon.Symbol.PLUS, teal).apply {
                    contentDescription = s(R.string.place_add)
                }
            clickable(plus) { action(Action.SAVE) }
            row.addView(plus, LinearLayout.LayoutParams(dp(40), dp(40)))
        }
        subpage.addView(row)
    }

    fun loading(routes: Boolean) {
        page = if (routes) Page.ROUTES else Page.PLACES
        pageHeader(if (routes) R.string.routes_title else R.string.places_title, !routes)
        val scroll = ScrollView(context)
        val body = column().apply { setPadding(dp(12), 0, dp(12), dp(8)) }
        body.addView(label(s(R.string.loading)))
        scroll.addView(body)
        subpage.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        listBody = body
    }

    fun choices(routes: Boolean, entries: List<Entry>, selected: (String) -> Unit) {
        if (page != if (routes) Page.ROUTES else Page.PLACES) return
        val body = listBody ?: return
        body.removeAllViews()
        if (entries.isEmpty()) body.addView(label(s(R.string.no_entries)))
        entries.forEach { entry ->
            val row =
                LinearLayout(context).apply {
                    gravity = Gravity.CENTER_VERTICAL
                    setPadding(0, dp(4), 0, dp(4))
                }
            val icon =
                OverlayIcon(
                    context,
                    if (routes) OverlayIcon.Symbol.ROUTE else OverlayIcon.Symbol.PLACE,
                    teal,
                )
            row.addView(icon, LinearLayout.LayoutParams(dp(30), dp(34)))
            val text = column().apply { setPadding(dp(6), 0, 0, 0) }
            text.addView(label(entry.name, 13f))
            text.addView(label(entry.detail, 11f, teal))
            row.addView(text, LinearLayout.LayoutParams(0, -2, 1f))
            clickable(row) { selected(entry.id) }
            body.addView(row, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(5) })
        }
    }

    fun askName(saved: (String) -> Unit) {
        page = Page.NAME
        pageHeader(R.string.save_position)
        val body = column().apply { setPadding(dp(14), dp(8), dp(14), dp(8)) }
        val input =
            EditText(context).apply {
                hint = s(R.string.place_name)
                isSingleLine = true
                textSize = 14f
            }
        body.addView(input, LinearLayout.LayoutParams(-1, dp(48)))
        val save = label(s(R.string.save), 14f, teal).apply { gravity = Gravity.CENTER }
        clickable(save) {
            val name = input.text.toString().trim()
            if (name.isEmpty()) input.error = s(R.string.enter_name)
            else {
                saved(name)
                showSettings()
            }
        }
        body.addView(save, LinearLayout.LayoutParams(-1, dp(44)))
        subpage.addView(body)
        setFocusable(true)
        input.requestFocus()
        input.post {
            if (page == Page.NAME)
                context
                    .getSystemService(InputMethodManager::class.java)
                    .showSoftInput(input, InputMethodManager.SHOW_IMPLICIT)
        }
    }

    private fun setFocusable(focusable: Boolean) {
        val root = panel ?: return
        if (!focusable)
            context
                .getSystemService(InputMethodManager::class.java)
                .hideSoftInputFromWindow(root.windowToken, 0)
        params.flags =
            if (focusable) params.flags and WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE.inv()
            else params.flags or WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
        windows.updateViewLayout(root, params)
    }

    fun reset(message: String? = null) {
        stick.reset()
        if (message != null) {
            details.text = message
            details.visibility = View.VISIBLE
        }
    }

    fun close() {
        panel?.let { root ->
            context
                .getSystemService(InputMethodManager::class.java)
                .hideSoftInputFromWindow(root.windowToken, 0)
            windows.removeView(root)
        }
        panel = null
    }
}
