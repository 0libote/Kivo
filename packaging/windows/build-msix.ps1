param(
  [ValidateSet("x64")]
  [string]$Architecture = "x64",
  [string]$CertificatePath = "",
  [string]$CertificatePassword = "",
  [string]$Publisher = "CN=0libote"
)

$ErrorActionPreference = "Stop"
$RepositoryRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Version = (Get-Content (Join-Path $RepositoryRoot "package.json") | ConvertFrom-Json).version
# MSIX Identity versions must be numeric X.Y.Z.W. Reject anything else here
# instead of shipping a manifest whose version silently failed to update.
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
  throw "package.json version '$Version' is not X.Y.Z, so no valid MSIX package version can be derived."
}
$PackageVersion = "$Version.0"
$Stage = Join-Path $RepositoryRoot "src-tauri\target\msix-stage"
$Output = Join-Path $RepositoryRoot "src-tauri\target\release\bundle\msix\Kivo_${Version}_$Architecture.msix"

function Get-WindowsSdkTool([string]$Name) {
  $kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
  $tool = Get-ChildItem -Path $kits -Filter $Name -Recurse |
    Where-Object { $_.FullName -match "\\x64\\" } |
    Sort-Object FullName -Descending |
    Select-Object -First 1
  if (-not $tool) { throw "$Name was not found. Install the Windows 11 SDK." }
  return $tool.FullName
}

Push-Location $RepositoryRoot
try {
  bun tauri build --no-bundle

  if (Test-Path $Stage) { Remove-Item $Stage -Recurse -Force }
  New-Item -ItemType Directory -Path (Join-Path $Stage "Assets") -Force | Out-Null
  New-Item -ItemType Directory -Path (Split-Path $Output) -Force | Out-Null

  Copy-Item "src-tauri\target\release\kivo.exe" (Join-Path $Stage "Kivo.exe")
  $manifest = Get-Content "packaging\windows\AppxManifest.xml" -Raw
  # Fail loudly when the placeholders drift: a silent no-op here previously
  # shipped MSIX packages with a stale identity version or publisher.
  $manifest = $manifest -replace 'Version="\d+\.\d+\.\d+\.\d+"', "Version=`"$PackageVersion`""
  if ($manifest -notmatch [regex]::Escape("Version=`"$PackageVersion`"")) {
    throw "AppxManifest.xml has no numeric Identity Version to stamp with $PackageVersion."
  }
  $manifest = $manifest -replace 'Publisher="CN=[^"]*"', "Publisher=`"$Publisher`""
  if ($manifest -notmatch [regex]::Escape("Publisher=`"$Publisher`"")) {
    throw "AppxManifest.xml has no Publisher to stamp with $Publisher."
  }
  Set-Content -Path (Join-Path $Stage "AppxManifest.xml") -Value $manifest -Encoding UTF8
  Copy-Item "src-tauri\icons\StoreLogo.png" (Join-Path $Stage "Assets\StoreLogo.png")
  Copy-Item "src-tauri\icons\Square44x44Logo.png" (Join-Path $Stage "Assets\Square44x44Logo.png")
  Copy-Item "src-tauri\icons\Square150x150Logo.png" (Join-Path $Stage "Assets\Square150x150Logo.png")

  $makeAppx = Get-WindowsSdkTool "makeappx.exe"
  & $makeAppx pack /d $Stage /p $Output /o

  if ($CertificatePath) {
    $signTool = Get-WindowsSdkTool "signtool.exe"
    & $signTool sign /fd SHA256 /f $CertificatePath /p $CertificatePassword $Output
  } else {
    Write-Warning "Created an unsigned MSIX. Sign it with a certificate whose subject matches the manifest Publisher."
  }

  Write-Output $Output
} finally {
  Pop-Location
}
