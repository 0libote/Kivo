param([Parameter(Mandatory = $true)][string]$InstallerDirectory)
$ErrorActionPreference = 'Stop'
$installers = @(Get-ChildItem "$InstallerDirectory/*-setup.exe")
if ($installers.Count -ne 1) { throw 'Expected exactly one Windows installer.' }
$root = Join-Path $env:RUNNER_TEMP ("kivo-smoke-" + [guid]::NewGuid().ToString('N'))
$install = Join-Path $root 'app'
$reports = Join-Path $root 'reports'
New-Item -ItemType Directory -Path $reports -Force | Out-Null
$previous = $env:KIVO_SMOKE_DIRECTORY
$app = $null
try {
    $setup = Start-Process $installers[0].FullName -ArgumentList '/S',"/D=$install" -Wait -PassThru
    if ($setup.ExitCode -ne 0) { throw "Installer failed: $($setup.ExitCode)" }
    $binary = Join-Path $install 'kivo.exe'
    if (-not (Test-Path $binary)) { throw 'Installed application is missing.' }
    $env:KIVO_SMOKE_DIRECTORY = $reports
    $app = Start-Process $binary -PassThru
    $deadline = (Get-Date).AddSeconds(90)
    $surfaces = @('settings', 'onboarding', 'flow-bar', 'writing-tools')
    while ((Get-Date) -lt $deadline) {
        $app.Refresh()
        if ($app.HasExited) { throw "Kivo exited during startup: $($app.ExitCode)" }
        $ready = @($surfaces | Where-Object { Test-Path (Join-Path $reports "$_.json") })
        if ($ready.Count -eq $surfaces.Count) { break }
        Start-Sleep -Milliseconds 500
    }
    foreach ($surface in $surfaces) {
        $path = Join-Path $reports "$surface.json"
        if (-not (Test-Path $path)) { throw "The $surface WebView did not report a rendered frontend." }
        $report = Get-Content $path | ConvertFrom-Json
        if ($report.surface -ne $surface -or -not $report.version) { throw "Invalid $surface readiness report." }
    }
    "Windows installer, launch, settings hydration, and all four WebViews passed." | Out-File $env:GITHUB_STEP_SUMMARY -Append
} finally {
    if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
    $env:KIVO_SMOKE_DIRECTORY = $previous
    $uninstaller = Join-Path $install 'uninstall.exe'
    if (Test-Path $uninstaller) {
        $uninstall = Start-Process $uninstaller -ArgumentList '/S' -Wait -PassThru
        if ($uninstall.ExitCode -ne 0) { throw "Uninstall failed: $($uninstall.ExitCode)" }
    }
    Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
}
