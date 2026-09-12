param(
    [Parameter(Mandatory)][string]$Serial,
    [string]$Adb = 'D:/Android/Sdk/platform-tools/adb.exe',
    [ValidateSet('basic', 'continuous', 'restore', 'route')][string]$Mode = 'basic',
    [switch]$PreserveConfig
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$package = 'me.idk.justlocation.probe'
function Device([string]$Command) {
    $result = & $Adb -s $Serial shell $Command
    if ($LASTEXITCODE -ne 0) { throw "Device command failed: $Command" }
    return ($result -join "`n").Trim()
}
function Request($Frame) {
    $json = $Frame | ConvertTo-Json -Depth 8 -Compress
    $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))
    $response = Device "su -c '/data/adb/modules/justlocation/bin/justlocationd request $encoded'" | ConvertFrom-Json
    if (!$response.ok) { throw $response.error }
    return $response.state
}
function Screen {
    Device 'uiautomator dump /data/local/tmp/justlocation-ui.xml' | Out-Null
    $xml = Device 'cat /data/local/tmp/justlocation-ui.xml'
    Set-Content -LiteralPath (Join-Path $projectRoot 'build/phone-location-ui.xml') -Value $xml -Encoding utf8
    return [xml]$xml
}
function Tap([string]$Label) {
    $node = (Screen).SelectSingleNode("//node[@package='$package' and @class='android.widget.Button' and @text='$Label']")
    if (!$node -or $node.bounds -notmatch '^\[(\d+),(\d+)\]\[(\d+),(\d+)\]$') { throw "Button unavailable: $Label (unlock the phone and show the check app)" }
    $x = [int](([int]$Matches[1] + [int]$Matches[3]) / 2)
    $y = [int](([int]$Matches[2] + [int]$Matches[4]) / 2)
    Device "input tap $x $y" | Out-Null
}
function ReadLocation([string]$Button, [string]$Stage, [string]$Expected, [bool]$Present = $true) {
    Tap '清空结果'
    Tap $Button
    Start-Sleep -Seconds 2
    $screen = Screen
    $texts = ($screen.SelectNodes("//node[@package='$package' and @text]") | ForEach-Object { $_.text }) -join "`n"
    Set-Content -LiteralPath (Join-Path $projectRoot "build/phone-location-$Stage.txt") -Value $texts -Encoding utf8
    if (!$texts.Contains('最近位置 ·') -and !$texts.Contains('单次定位 ·')) { throw "$Stage returned no Android location result" }
    if ($texts.Contains($Expected) -ne $Present) { throw "$Stage did not meet the expected location result; see build/phone-location-$Stage.txt" }
    if ($Stage -eq 'current' -and !$texts.Contains('单次定位已结束')) { throw 'Single location callbacks finished, but the check app still shows a waiting status' }
    Write-Output "PASS: $Stage"
}
function StartSession($Position, [string]$Target) {
    Request @{version=1; op='start'; config=@{position=$Position; scope=@{mode='apps'; packages=@($Target)}}} | Out-Null
    Start-Sleep -Seconds 2
}
$before = Device 'pidof system_server'
if ((Device 'dumpsys user') -notmatch '0=RUNNING_UNLOCKED') { throw 'Unlock the phone once after reboot before running this test' }
$state = Request @{version=1; op='status'}
if (!$state.location_hook_ready -or !$state.hook_connected) { throw 'System hooks are not ready' }
if ($state.requested_active) { throw 'Stop the existing session before running this test' }
$savedConfig = $state.config
if ($state.config -and !$PreserveConfig) {
    $saved = $state.config
    $testScope = $saved.scope.mode -eq 'apps' -and $saved.scope.packages.Count -eq 1 -and $saved.scope.packages[0] -in @($package, 'me.idk.justlocation.unselected.test')
    $testPoint = (($saved.position.latitude -eq 31.2 -and $saved.position.longitude -eq 121.5) -or ($saved.position.latitude -eq 31.3 -and $saved.position.longitude -eq 121.6)) -and $saved.position.altitude -eq 12 -and $saved.position.accuracy -eq 5 -and $saved.position.speed -eq 0 -and $saved.position.bearing -eq 0
    if (!$testScope -or !$testPoint) { throw 'This integration test requires an unused configuration or its previous test coordinates' }
}
if ((Device 'cmd location is-location-enabled') -ne 'true') { throw 'Enable location before running the integration test' }
$point = @{latitude=31.2; longitude=121.5; altitude=12; accuracy=5; speed=0; bearing=0}
try {
    Device 'input keyevent KEYCODE_WAKEUP' | Out-Null
    Device "am start -W -n $package/.MainActivity" | Out-Null
    if ($Mode -eq 'route') {
        $destination = @{latitude=31.3; longitude=121.5; altitude=12; accuracy=5; speed=0; bearing=0}
        Request @{version=1; op='start_route'; route=@{points=@($point, $destination); speed=30}; scope=@{mode='apps'; packages=@($package)}} | Out-Null
        Start-Sleep -Seconds 3
        $paused = Request @{version=1; op='pause_route'}
        $first = $paused.config.position
        if ($first.latitude -le 31.2 -or $first.latitude -ge 31.3 -or !$paused.route.paused) { throw 'Route did not advance and pause' }
        Start-Sleep -Seconds 2
        $expected = '{0:F6}, {1:F6}' -f $first.latitude, $first.longitude
        ReadLocation '读取最近位置' 'route-paused-output' $expected
        $still = Request @{version=1; op='status'}
        if ($still.config.position.latitude -ne $first.latitude) { throw 'Paused route moved' }
        Request @{version=1; op='resume_route'} | Out-Null
        Start-Sleep -Seconds 3
        $resumed = Request @{version=1; op='pause_route'}
        if ($resumed.config.position.latitude -le $first.latitude) { throw 'Resumed route did not move' }
        Start-Sleep -Seconds 2
        $expected = '{0:F6}, {1:F6}' -f $resumed.config.position.latitude, $resumed.config.position.longitude
        ReadLocation '请求单次定位' 'route-resumed-output' $expected
        Write-Output 'PASS: route movement, pause, resume and actual Android outputs'
        return
    }
    if ($Mode -in @('continuous', 'restore')) {
        StartSession $point $package
        Tap '清空结果'
        $clear = (Screen).SelectSingleNode("//node[@package='$package' and @class='android.widget.Button' and @text='清空结果']")
        if ($clear.bounds -notmatch '^\[(\d+),(\d+)\]\[(\d+),(\d+)\]$') { throw 'Clear button unavailable' }
        $clearX = [int](([int]$Matches[1] + [int]$Matches[3]) / 2)
        $clearY = [int](([int]$Matches[2] + [int]$Matches[4]) / 2)
        Tap '开始接收持续定位'
        Start-Sleep -Seconds 10
        if ($Mode -eq 'restore') {
            Request @{version=1; op='stop'} | Out-Null
            Start-Sleep -Seconds 4
            # Keep the same subscription; clear only already-delivered UI results.
            Device "input tap $clearX $clearY" | Out-Null
            Start-Sleep -Seconds 8
        } else {
            $point.latitude = 31.3; $point.longitude = 121.6
            Request @{version=1; op='update'; position=$point} | Out-Null
            Start-Sleep -Seconds 15
        }
        # Pause callbacks before dumping: frequent UI updates prevent idle detection.
        Device 'input keyevent KEYCODE_HOME' | Out-Null
        Start-Sleep -Seconds 2
        Device "am start -W -n $package/.MainActivity" | Out-Null
        $texts = ((Screen).SelectNodes("//node[@package='$package' and @text]") | ForEach-Object { $_.text }) -join "`n"
        Set-Content -LiteralPath (Join-Path $projectRoot "build/phone-location-$Mode.txt") -Value $texts -Encoding utf8
        if (!$texts.Contains('持续回调')) { throw 'No continuous callbacks received' }
        if ($Mode -eq 'restore') {
            if ($texts.Contains('31.200000, 121.500000')) { throw 'Test coordinates still delivered after stopping simulation' }
            Write-Output 'PASS: existing subscription resumes original output after simulation stops'
        } else {
            if (!$texts.Contains('31.200000, 121.500000') -or !$texts.Contains('31.300000, 121.600000')) { throw 'Continuous callbacks did not deliver both test positions' }
            Write-Output 'PASS: continuous callbacks and update during subscription'
        }
        return
    }
    ReadLocation '读取最近位置' 'baseline' '31.200000, 121.500000' $false
    StartSession $point $package
    ReadLocation '读取最近位置' 'start' '31.200000, 121.500000'
    ReadLocation '请求单次定位' 'current' '31.200000, 121.500000'
    $point.latitude = 31.3; $point.longitude = 121.6
    Request @{version=1; op='update'; position=$point} | Out-Null
    Start-Sleep -Seconds 2
    ReadLocation '读取最近位置' 'update' '31.300000, 121.600000'
    Request @{version=1; op='stop'} | Out-Null
    Start-Sleep -Seconds 2
    ReadLocation '读取最近位置' 'stop' '31.300000, 121.600000' $false
    StartSession $point 'me.idk.justlocation.unselected.test'
    ReadLocation '读取最近位置' 'excluded' '31.300000, 121.600000' $false
} finally {
    Request @{version=1; op='stop'} | Out-Null
    Device "am force-stop $package" | Out-Null
    if ($PreserveConfig -and $savedConfig) {
        Request @{version=1; op='start'; config=$savedConfig} | Out-Null
        Request @{version=1; op='stop'} | Out-Null
        Write-Output 'Original saved configuration restored; simulation stopped'
    }
    Device 'rm -f /data/local/tmp/justlocation-ui.xml' | Out-Null
    if ((Device 'pidof system_server') -ne $before) { throw 'system_server restarted during the test' }
    Write-Output 'Session stopped; check app closed; system_server unchanged'
}
