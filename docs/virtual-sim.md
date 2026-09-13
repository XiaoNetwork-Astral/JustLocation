# Virtual subscriptions

Virtual subscriptions supply Android subscription metadata in confirmed empty SIM slots. They do not create a cellular connection, phone number, IMSI, ICCID or eSIM profile. A physical card's activation, PIN, calls, data connection and default choices remain controlled by Android.

Use the matching daemon, native module and bridge DEX, then reboot. Updating only the joystick APK does not install these hooks. Android 15 is the current implementation target.

## Configure

The examples use the `justlocation` alias from the [README](../README.md).

```sh
justlocation sim virtual upsert --slot 0 --mcc 460 --mnc 01 --carrier 'Test carrier'
justlocation sim virtual upsert --slot 1 --mcc 460 --mnc 11 --carrier 'Second carrier'
justlocation sim virtual default 1
justlocation scope set --feature sim --app com.example.target
justlocation sim operator true
justlocation start --lat 39.907333 --lon 116.391083 --app com.example.target
justlocation sim get
justlocation status
justlocation stop
```

Slots are zero-based. Upsert allocates an ID in `1900000000..=2000000001`; editing a slot and restarting retain that ID. Remove and recreate is a new configuration. IDs conflicting with a physical card or configured physical template are blocked instead of silently reassigned.

`sim virtual default` without a slot selects the lowest available virtual slot. The configured preference applies when there are no active real subscriptions. With real subscriptions present, Android's defaults, including the SMS “ask each time” choice, stay unchanged.

```sh
justlocation sim virtual upsert --slot 0 --mcc 460 --mnc 01 --carrier 'Test carrier' --enabled false
justlocation sim virtual remove 1
```

Disabling or removing the final enabled configured subscription also turns off the SIM and cell simulation switches. Adding a subscription does not enable those switches automatically. `sim set --input FILE` can configure the same `telephony.virtual_sim` object directly.

## Availability and scope

The phone process reports actual active subscriptions and the active modem count. An enabled virtual slot is available only when that report is fresh, the slot exists and no real active subscription occupies it. Unknown modem/subscription state remains unavailable. Inserting or enabling a real card blocks its virtual slot on the next phone report; it does not change the saved virtual ID or remove the virtual configuration.

`status.telephony_output` includes `virtual_ids`, `virtual_default_id`, `has_real_subscriptions` and `virtual_blocked`. Blocking reasons are `sim_disabled`, `modems_unknown`, `slot_unavailable`, `subscriptions_unknown`, `slot_occupied` or `id_conflict`. Output is absent when simulation is stopped.

The SIM app list governs subscription lists, individual records, mappings, defaults, card state and subscription-change notifications. Cell data follows the effective position/route list as described in [application scopes](scopes.md). Original Android permission, AppOps and user/profile checks run before adding records. A virtual record needs general phone-state permission; privileges from a different real carrier do not grant it. Queries without a package argument use Binder UID ownership; an ambiguous shared UID receives virtual mappings only if every associated package is selected. Internal system and phone work reads the actual subscription database.

Virtual records are ordinary local subscriptions. They are not inserted into embedded or opportunistic lists, and ICCID queries do not invent identifiers. Unknown IDs/slots keep the original result. A virtual card reports loaded state through the phone service, which Android exposes as READY/PRESENT through the corresponding public APIs; this does not assert network registration or service availability.

Operator-name properties are shared by Android. They change only with `scope set --feature sim --all`; a selected app list retains real property values while scoped `SubscriptionInfo` and `ServiceState` fields change. Only configured slots are replaced, preserving other slots' operator fields. Stop restores captured originals, including empty no-SIM values, and filters echoes of the daemon's own writes.

## Cache changes and diagnostics

The phone invalidates `SubscriptionManager` caches after applying a changed snapshot. It then acknowledges `virtual_sim_version` as `virtual_sim_applied`. System-server subscription listeners receive the change after that acknowledgement, so their next query can see the new model. Registered listeners are re-evaluated after list edits and receive restoration after leaving scope, stopping or snapshot expiry. The shared expiry is 20 seconds; explicit stop normally restores on the next heartbeats.

`virtual_sim_query_hook_ready` and `virtual_sim_callback_hook_ready` report query installation and registry availability separately. Check both plus the publication acknowledgement when diagnosing an active session. They describe installed channels, not a completed cross-application acceptance test.

## Validation

Host tests cover no/one/two real subscriptions, stable IDs, conflicts, modem limits, defaults, permission denial, scope changes, publication ordering and empty-property restoration. `TelephonyObjectChecks -e virtual_only true` checks Android objects, parceling and the actual ROM's required signatures. `integration/device/run-virtual-sim.mjs ADB SERIAL ARM64_BINARY` exercises an isolated daemon with fixture phone reports, including CLI edits and restart persistence; it does not feed the installed module.

Real no-SIM acceptance remains separate: install the matching module, arrange removal/deactivation of real subscriptions with the device owner, and compare selected and unselected apps across start, list edits, stop and process loss. Fixture and object checks do not replace that test.
