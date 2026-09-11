$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$adbPath = Join-Path $projectRoot 'build/avd-sdk/platform-tools/adb.exe'
$consolePath = Join-Path $projectRoot 'tools/avd-console.mjs'
$package = 'me.idk.justlocation.companion'
function DeviceCommand([string[]]$Arguments) {
    $output = & $adbPath -s emulator-5580 @Arguments
    if ($LASTEXITCODE -ne 0) { throw "AVD command failed: $($Arguments -join ' ')" }
    return ($output -join "`n").Trim()
}
function Screen {
    DeviceCommand @('shell', 'rm -f /data/local/tmp/justlocation-ui.xml') | Out-Null
    DeviceCommand @('shell', 'uiautomator dump /data/local/tmp/justlocation-ui.xml') | Out-Null
    $xml = DeviceCommand @('shell', 'cat /data/local/tmp/justlocation-ui.xml')
    Set-Content -LiteralPath (Join-Path $projectRoot 'build/avd-location-last.xml') -Value $xml -Encoding utf8
    return [xml]$xml
}
function Tap([string]$Label) {
    $node = (Screen).SelectSingleNode("//node[@class='android.widget.Button' and @text='$Label']")
    if (!$node -or $node.bounds -notmatch '^\[(\d+),(\d+)\]\[(\d+),(\d+)\]$') { throw "Button unavailable: $Label" }
    $x = [int]( ([int]$Matches[1] + [int]$Matches[3]) / 2 )
    $y = [int]( ([int]$Matches[2] + [int]$Matches[4]) / 2 )
    DeviceCommand @('shell', "input tap $x $y") | Out-Null
}
function Fix([string]$Longitude, [string]$Latitude) {
    & node $consolePath geo fix $Longitude $Latitude 12 8 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'AVD GPS injection failed.' }
}
function WaitFix([double]$Latitude, [double]$Longitude) {
    for ($attempt = 0; $attempt -lt 15; $attempt++) {
        $state = DeviceCommand @('shell', 'dumpsys location')
        $registration = $state.LastIndexOf('gps provider +registration')
        $delivered = $registration -ge 0 -and $state.Substring($registration).Contains('gps provider delivered location')
        if ($state -match 'last location=Location\[gps (-?[\d.]+),(-?[\d.]+)' -and
            [Math]::Abs([double]$Matches[1] - $Latitude) -lt 0.00001 -and
            [Math]::Abs([double]$Matches[2] - $Longitude) -lt 0.00001 -and $delivered) { return }
        Start-Sleep -Seconds 1
    }
    throw 'AVD GPS provider did not receive the requested fix.'
}
function PauseCallbacks {
    # Continuous updates prevent uiautomator's idle wait from completing.
    # onStop removes the listeners; returning preserves the received results.
    Start-Sleep -Seconds 2
    DeviceCommand @('shell', 'input keyevent KEYCODE_HOME') | Out-Null
    Start-Sleep -Seconds 2
    DeviceCommand @('shell', "am start -n $package/.MainActivity") | Out-Null
}
function ExpectText([string]$Expected) {
    for ($attempt = 0; $attempt -lt 5; $attempt++) {
        $texts = (Screen).SelectNodes('//node[@text]') | ForEach-Object { $_.text }
        if (($texts -join "`n").Contains($Expected)) { Write-Output "PASS: $Expected"; return }
        Start-Sleep -Milliseconds 500
    }
    throw "UI did not show: $Expected"
}
function ExpectCoordinates([double]$Latitude, [double]$Longitude) {
    # Emulator GNSS coordinates can be rounded before reaching LocationManager.
    $text = ((Screen).SelectNodes('//node[@text]') | ForEach-Object { $_.text }) -join "`n"
    foreach ($point in [regex]::Matches($text, '(-?\d+\.\d+), (-?\d+\.\d+)')) {
        if ([Math]::Abs([double]$point.Groups[1].Value - $Latitude) -lt 0.00001 -and
            [Math]::Abs([double]$point.Groups[2].Value - $Longitude) -lt 0.00001) {
            Write-Output "PASS: Android callback near $Latitude, $Longitude (within 0.00001 degrees)"
            return
        }
    }
    throw "UI did not receive coordinates near $Latitude, $Longitude"
}
if ((& node $consolePath avd name).Trim() -ne 'JustLocation_API35') { throw 'Wrong AVD.' }
if ((DeviceCommand @('shell', 'getprop sys.boot_completed')) -ne '1') { throw 'AVD has not booted.' }
$before = DeviceCommand @('shell', 'pidof system_server')
try {
    DeviceCommand @('shell', "pm grant $package android.permission.ACCESS_FINE_LOCATION") | Out-Null
    DeviceCommand @('shell', "pm grant $package android.permission.ACCESS_COARSE_LOCATION") | Out-Null
    DeviceCommand @('shell', "am force-stop $package") | Out-Null
    DeviceCommand @('shell', "am start -n $package/.MainActivity") | Out-Null
    Tap '开始接收持续定位'
    Fix '121.5' '31.2'
    WaitFix 31.2 121.5
    PauseCallbacks
    ExpectCoordinates 31.2 121.5
    ExpectText '持续回调'
    Tap '清空结果'
    Tap '开始接收持续定位'
    Fix '121.6' '31.3'
    WaitFix 31.3 121.6
    PauseCallbacks
    ExpectCoordinates 31.3 121.6
    Tap '停止接收'
    Tap '清空结果'
    Fix '121.7' '31.4'
    Start-Sleep -Seconds 2
    ExpectText '暂无结果'
    ExpectText '已停止接收'
    Tap '读取最近位置'
    ExpectText '最近位置 · gps'
    Tap '清空结果'
    Tap '请求单次定位'
    Fix '121.8' '31.5'
    ExpectText '单次定位 · gps'
    Write-Output 'PASS: AVD Android location callbacks, changed fix, stop, last and current APIs'
} finally {
    DeviceCommand @('shell', "am force-stop $package") | Out-Null
    DeviceCommand @('shell', 'rm -f /data/local/tmp/justlocation-ui.xml') | Out-Null
    if ((DeviceCommand @('shell', 'pidof system_server')) -ne $before) { throw 'AVD system_server restarted.' }
}
