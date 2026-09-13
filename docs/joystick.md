# Floating joystick

Open with `justlocation joystick open --speed 1.5`. Speed is in metres per second. The module service and location hook must be ready; allow JustLoystick Root access in KernelSU and permission to draw over other apps.

The circular joystick stays visible when settings are collapsed. Tap the short toolbar to expand or collapse the settings card; drag its menu handle to move the window. Settings scroll within the card while its simulation switch and close action stay at the bottom. Location and route lists open inside the same card, with a back arrow to return. The interface follows the system language (English or Chinese).

- **Manual direction:** screen up means geographic north. Drag distance controls the fraction of the selected maximum speed. Releasing the stick stops joystick movement; the centre returns with a short animation while the small pointer retains the last direction. Pointer and return animations only affect drawing.
- **Lock direction:** enable before or after choosing a direction. Releasing the stick keeps the latest direction and drag strength; dragging again changes them. Turning the lock off stops movement.
- **Follow phone:** tap the direction setting to start continuous movement at the selected speed, following the screen's top edge relative to magnetic north. Tap again to stop, or move the stick to return to manual control. The compass needs fresh, reliable sensor samples; hold the device reasonably flat. Magnetic north is not corrected using the simulated location. If samples become unavailable, movement pauses until valid readings return.
- **Speed:** use the slider or walking, running, cycling, driving and flying presets. The value is a joystick maximum, not a route's playback speed.
- **Steps / GPS signal:** toggle the existing backend settings. Other step parameters and the separate NMEA setting are retained.
- **Switch location / route:** select a saved library entry. Switching ends the current simulation before starting the selected entry with the configured app scope. If starting it fails, the error is shown and simulation stays stopped.
- **Save current position:** name and save the current simulated position to the shared location library. The plus icon in the location list opens the same form.
- **Routes:** the settings card shows distance progress and pause, resume and stop actions during playback. Stop the route before moving the joystick.

Collapsing settings preserves movement. Dragging the window, turning the screen off, changing display orientation or closing the joystick releases joystick movement. Closing the joystick leaves a stationary simulation or independently running route in place; use the simulation switch or `justlocation stop` to stop that session. Movement commands renew a two-second backend lease, so a killed or disconnected joystick cannot keep renewing an old direction.
