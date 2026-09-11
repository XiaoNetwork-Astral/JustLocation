param(
    [ValidateSet('start', 'stop', 'status')][string]$Action = 'status',
    [switch]$Visible
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$sdkPath = Join-Path $projectRoot 'build\avd-sdk'
$adbPath = Join-Path $sdkPath 'platform-tools\adb.exe'
$emulatorPath = Join-Path $sdkPath 'emulator\emulator.exe'
$env:ANDROID_HOME = $sdkPath
$env:ANDROID_USER_HOME = Join-Path $projectRoot 'build\android-user'
$env:ANDROID_AVD_HOME = Join-Path $projectRoot 'build\avd-home'
$env:TEMP = Join-Path $projectRoot 'build\tmp'
$env:TMP = $env:TEMP
if (!(Test-Path -LiteralPath $emulatorPath)) { throw 'AVD SDK is not installed under build/avd-sdk.' }
switch ($Action) {
    'status' {
        & $adbPath -s emulator-5580 get-state
        if ($LASTEXITCODE -eq 0) { & $adbPath -s emulator-5580 shell getprop sys.boot_completed }
    }
    'stop' {
        & node (Join-Path $PSScriptRoot 'avd-console.mjs') kill
        if ($LASTEXITCODE -ne 0) { throw 'AVD shutdown failed.' }
    }
    'start' {
        $devices = & $adbPath devices
        if ($devices -match '^emulator-5580\s') { throw 'AVD port 5580 is already in use; check its status first.' }
        $arguments = @('-avd', 'JustLocation_API35', '-port', '5580', '-no-audio', '-no-snapshot', '-gpu', 'software', '-memory', '2048')
        if (!$Visible) { $arguments += '-no-window' }
        Start-Process -FilePath $emulatorPath -ArgumentList $arguments -WindowStyle Hidden `
            -RedirectStandardOutput (Join-Path $projectRoot 'build\avd-stdout.log') `
            -RedirectStandardError (Join-Path $projectRoot 'build\avd-stderr.log') | Out-Null
        Write-Output 'Started JustLocation_API35 on emulator-5580. Use tools/avd.ps1 status to check boot completion.'
    }
}
