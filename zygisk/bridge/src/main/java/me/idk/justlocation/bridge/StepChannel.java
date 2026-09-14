package me.idk.justlocation.bridge;

import android.content.Context;
import android.hardware.Sensor;
import android.hardware.SensorEvent;
import android.hardware.SensorEventListener;
import android.hardware.SensorManager;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.SystemClock;
import android.util.Log;

import java.lang.reflect.Method;
import java.util.ArrayList;

import org.json.JSONArray;
import org.json.JSONObject;

/** Supplies native step snapshots and a sensor event clock while simulation is active. */
final class StepChannel implements SensorEventListener {
    private SensorManager manager;
    private Sensor clock;
    private Handler handler;
    private int[] handles;
    private int[] types;
    private boolean registered;
    private boolean installed;
    private long received;
    private long lastError;
    private final float[] motionAccelerometer = new float[3];
    private final float[] motionGyroscope = new float[3];
    private double motionCadence;
    private boolean motionActive;

    private boolean prepare() throws Exception {
        if (installed)
            return true;
        Class<?> activityThread = Class.forName("android.app.ActivityThread");
        Object thread = activityThread.getMethod("currentActivityThread").invoke(null);
        if (thread == null)
            return false;
        Context context = (Context) activityThread.getMethod("getSystemContext").invoke(thread);
        manager = context.getSystemService(SensorManager.class);
        if (manager == null)
            return false;
        clock = manager.getDefaultSensor(Sensor.TYPE_ACCELEROMETER);
        Method getHandle = Sensor.class.getDeclaredMethod("getHandle");
        getHandle.setAccessible(true);
        ArrayList<Integer> ids = new ArrayList<>();
        ArrayList<Integer> kinds = new ArrayList<>();
        boolean counter = false, detector = false;
        for (Sensor sensor : manager.getSensorList(Sensor.TYPE_ALL)) {
            int type = sensor.getType();
            // Raw motion sensors are routed only while the daemon says the channel is generating;
            // the native side keeps the real events otherwise.
            if (type != Sensor.TYPE_STEP_COUNTER && type != Sensor.TYPE_STEP_DETECTOR
                    && type != Sensor.TYPE_ACCELEROMETER && type != Sensor.TYPE_GYROSCOPE)
                continue;
            ids.add((Integer) getHandle.invoke(sensor));
            kinds.add(type);
            counter |= type == Sensor.TYPE_STEP_COUNTER;
            detector |= type == Sensor.TYPE_STEP_DETECTOR;
        }
        if (!counter || !detector || clock == null || !BridgeEntry.installSteps())
            return false;
        handles = ids.stream().mapToInt(Integer::intValue).toArray();
        types = kinds.stream().mapToInt(Integer::intValue).toArray();
        HandlerThread worker = new HandlerThread("JustLocation-step-clock");
        worker.start();
        handler = new Handler(worker.getLooper());
        installed = true;
        return true;
    }

    void update(String response) {
        try {
            JSONObject reply = new JSONObject(response);
            JSONObject state = reply.getJSONObject("state");
            JSONObject settings = state.optJSONObject("steps");
            if (!state.optBoolean("requested_active") || settings == null
                    || !settings.optBoolean("enabled")) {
                stop();
                return;
            }
            if (!prepare())
                return;
            ScopeSelection scope = ScopeSelection.read(state, null);
            if (scope == null) {
                stop();
                return;
            }
            String[] packages = scope.packages().toArray(new String[0]);
            JSONObject count = state.getJSONObject("step_count");
            // The raw six-axis block comes from the daemon's motion model; this side only passes it
            // through, so the delivered samples cannot describe a different motion than the fix.
            readMotion(state.optJSONObject("motion_output"));
            // A continuous sensor supplies regular SensorService batches even when the phone is
            // stationary. Its values remain untouched; only subscribed step sensors are replaced.
            if (!registered)
                registered = manager.registerListener(this, clock, 20_000, 0, handler);
            BridgeEntry.updateSteps(registered, scope.all(), packages, count.getLong("total"),
                    count.getLong("epoch"), handles, types, motionActive, motionCadence,
                    motionAccelerometer, motionGyroscope);
            received = SystemClock.elapsedRealtime();
        } catch (Exception error) {
            stop();
            long now = SystemClock.elapsedRealtime();
            if (now - lastError > 30_000) {
                lastError = now;
                Log.w("JustLocation", "Cannot update step sensor channel", error);
            }
        }
    }

    /** Read one heartbeat's raw motion block; a missing or malformed block disables the channel. */
    private void readMotion(JSONObject motion) {
        motionActive = false;
        motionCadence = 0.0;
        if (motion == null)
            return;
        JSONArray accelerometer = motion.optJSONArray("accelerometer");
        JSONArray gyroscope = motion.optJSONArray("gyroscope");
        if (accelerometer == null || gyroscope == null || accelerometer.length() != 3
                || gyroscope.length() != 3)
            return;
        for (int axis = 0; axis < 3; axis++) {
            motionAccelerometer[axis] = (float) accelerometer.optDouble(axis, 0.0);
            motionGyroscope[axis] = (float) gyroscope.optDouble(axis, 0.0);
        }
        motionCadence = motion.optDouble("cadence", 0.0);
        motionActive = true;
    }

    void tick() {
        if (registered && SystemClock.elapsedRealtime() - received >= 20_000)
            stop();
    }

    void stop() {
        motionActive = false;
        motionCadence = 0.0;
        BridgeEntry.updateSteps(false, false, null, 0, 0, null, null, false, 0.0,
                motionAccelerometer, motionGyroscope);
        if (registered) {
            manager.unregisterListener(this);
            registered = false;
        }
    }

    @Override
    public void onSensorChanged(SensorEvent event) {}
    @Override
    public void onAccuracyChanged(Sensor sensor, int accuracy) {}
}
