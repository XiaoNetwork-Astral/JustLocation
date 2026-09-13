# Backups

`backup export` saves portable JustLocation data as versioned JSON. Add `--gzip` for a smaller file, especially when routes contain many points. Import detects either representation automatically.

The examples use the `justlocation` alias from the README.

```sh
justlocation backup export --output /sdcard/Download/justlocation.json
justlocation backup export --gzip --output /sdcard/Download/justlocation.json.gz
justlocation backup export --category places,routes --output /sdcard/Download/trips.json
```

## What is included

| Category   | Contents                                                                                                                                 |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `places`   | Saved places, names, pinned state, coordinates, metadata, and attached cells/Wi-Fi                                                       |
| `routes`   | Saved routes with every point, segment break, playback setting and provider geometry metadata                                            |
| `wifi`     | Saved Wi-Fi simulation targets and their enabled setting                                                                                 |
| `scopes`   | The separate position, route, Wi-Fi and SIM application rules                                                                            |
| `settings` | Last configured position, cell region, telephony and virtual subscriptions, GNSS, steps, realism, and portable cell-provider preferences |

Map keys, provider credentials, accounts, device identity, active simulation/recording sessions, step totals, downloaded cell datasets and app-local display preferences are excluded. Provider endpoints are included, but credentials embedded in an endpoint are rejected. Import retains the destination's OpenCellID key. A custom provider token is retained only when the endpoint is unchanged; changing the endpoint clears that token.

An exported route contains its full plan rather than a reference to a file on the source device. A route supports up to 100,000 points. Input and expanded JSON are limited to 64 MiB; export fewer categories or individual routes if the combined backup exceeds that size. Local library metadata has a separate 2 MiB limit.

## Preview and restore

```sh
justlocation --json backup import --input /sdcard/Download/justlocation.json.gz --preview
justlocation shutdown
justlocation --json backup import --input /sdcard/Download/justlocation.json.gz
justlocation service start
```

Restoring requires the JustLocation backend to be shut down so its in-memory settings cannot overwrite restored files. A phone reboot is unnecessary. Preview may run while the backend is running. Restoring a saved position does not start a simulation.

By default, all categories present in the file are selected. Use `--category` to restore a subset. Selecting a category absent from the file is an error.

Collections of places, routes and Wi-Fi targets are merged by default. `--replace` removes the destination collection only for selected categories before importing it. The four scope rules and functional settings are restored as complete configurations in either mode. Wi-Fi's enabled setting is also taken from the backup. Unselected categories retain their values.

```sh
justlocation --json backup import --input trips.json --category places,routes --conflicts skip --preview
justlocation --json backup import --input trips.json --category routes --replace --preview
```

ID collisions support `--conflicts rename` (the default), `skip`, or `error`. Places and routes share an ID namespace; Wi-Fi targets have their own namespace. A preview reports imported, skipped and removed counts, each conflict action, and scope changes. Newly proposed IDs are generated again on restore; a preview does not reserve them.

All selected data is validated before publication. File-write failures roll back the affected library, settings and provider files together. An interrupted restore is recovered before the next data command or backend start. Do not manually remove the private restore journal while recovery is pending.

## Format and compatibility

The current format is:

```json
{
  "format": "justlocation-backup",
  "version": 1,
  "categories": {
    "places": [],
    "routes": []
  }
}
```

Unselected categories are omitted. GZIP contains the same JSON schema, encoded without whitespace. Unsupported versions, invalid fields, corrupt compression and incomplete route plans are rejected before restoring any category.

The earlier `justlocation-library` version 1 and `justlocation` version 1 formats remain readable. Use `backup export --legacy --output library.json` to create a places/routes-only export for older JustLocation versions.

Fake Location `.bak` files are a separate migration source and are not accepted by this importer yet. The reviewed 1.5.2 implementation uses a UTF-8 JSON `header`/`data` container whose categories contain storage key/value pairs. That is different from both this format and a single-place S code. Place S codes can already be imported with `place import`; complete `.bak` migration still needs a controlled export to verify the stored list encoding and conversions.

## 中文速览

默认使用自己的版本化 JSON；长路线可加 `--gzip` 压缩。支持按 `places,routes,wifi,scopes,settings` 分类备份与恢复，路线完整保存点和分段，地点保留附件与元数据。Key、凭据、运行会话、步数累计、离线基站数据集和 App 本地显示偏好不在备份中。

先用 `backup import --preview` 查看影响，再 `shutdown` 后导入；只需停止本项目后台，无需重启手机。默认合并地点、路线和 Wi-Fi 集合；`--replace` 仅替换所选集合。范围和功能设置作为完整配置恢复。ID 冲突可选 `rename`、`skip`、`error`，未选类别保留。失败会回滚，中断后由下一次数据操作或后台启动恢复。

继续支持本项目旧备份。Fake Location 整体 `.bak` 尚未接入；它仅作为迁移来源，不决定我们的备份格式。目前可通过 `place import` 迁移单地点 S 码。
