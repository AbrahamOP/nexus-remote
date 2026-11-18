[CmdletBinding()]
param(
    [string]$OutputDir,
    [string]$SourceArchive,
    [switch]$Offline
)

$ErrorActionPreference = 'Stop'
$version = '3.5.9'
$expectedSha256 = '603f5602e2eef00d77fbd429d34dcd5822bb301757a1bc9cdb24c670f1eb859a'
$fileName = "openssl-$version.tar.gz"
$releaseUrl = "https://github.com/openssl/openssl/releases/download/openssl-$version/$fileName"
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if (-not $OutputDir) { $OutputDir = Join-Path $projectRoot ".local-build\openssl-$version" }
$OutputDir = [IO.Path]::GetFullPath($OutputDir)
$installDir = Join-Path $OutputDir 'install'
$sourceRoot = Join-Path $OutputDir 'source'
$sourceDir = Join-Path $sourceRoot "openssl-$version"
$archive = if ($SourceArchive) { [IO.Path]::GetFullPath($SourceArchive) } else { Join-Path $OutputDir $fileName }

if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitOperatingSystem) {
    throw 'OpenSSL must be built on Windows x64.'
}
if ($env:VSCMD_ARG_TGT_ARCH -and $env:VSCMD_ARG_TGT_ARCH -ne 'x64') {
    throw 'Use an x64 Developer PowerShell for Visual Studio.'
}
foreach ($tool in @('cl.exe', 'link.exe', 'nmake.exe', 'perl.exe', 'tar.exe')) {
    if (-not (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)) {
        throw "Missing $tool. Use an x64 Developer PowerShell with MSVC, Perl and tar.exe on PATH."
    }
}
if (Test-Path -LiteralPath $sourceDir) {
    throw "An extracted OpenSSL source tree already exists at $sourceDir. Use a fresh -OutputDir; this script will not reuse or overwrite a possibly modified source tree."
}

New-Item -ItemType Directory -Path $OutputDir, $sourceRoot -Force | Out-Null
if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) {
    if ($Offline) { throw "Archive missing in offline mode: $archive" }
    Write-Host "Downloading OpenSSL $version from the official release..."
    Invoke-WebRequest -Uri $releaseUrl -OutFile $archive -UseBasicParsing
}
$actualSha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualSha256 -ne $expectedSha256) {
    throw "OpenSSL archive SHA-256 mismatch. Expected $expectedSha256, got $actualSha256."
}
Write-Host "OpenSSL archive SHA-256 verified: $actualSha256"
$localArchive = Join-Path $OutputDir $fileName
if (-not [string]::Equals($archive, $localArchive, [StringComparison]::OrdinalIgnoreCase)) {
    Copy-Item -LiteralPath $archive -Destination $localArchive
    $archive = $localArchive
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expectedSha256) {
        throw 'Copied OpenSSL archive SHA-256 mismatch.'
    }
}

Write-Host 'Extracting OpenSSL source...'
& tar.exe -xzf $archive -C $sourceRoot
if ($LASTEXITCODE -ne 0) { throw "OpenSSL extraction failed (exit $LASTEXITCODE)." }
if (-not (Test-Path -LiteralPath (Join-Path $sourceDir 'Configure') -PathType Leaf)) {
    throw 'OpenSSL Configure is missing after extraction.'
}

$perl = (Get-Command perl.exe -CommandType Application).Source
$nmake = (Get-Command nmake.exe -CommandType Application).Source
Push-Location -LiteralPath $sourceDir
try {
    Write-Host 'Configuring static OpenSSL for MSVC x64...'
    # OpenSSL's VC-WIN64A configuration selects /MT automatically for no-shared.
    & $perl Configure VC-WIN64A no-shared no-asm "--prefix=$installDir" "--openssldir=$(Join-Path $installDir 'ssl')"
    if ($LASTEXITCODE -ne 0) { throw "OpenSSL Configure failed (exit $LASTEXITCODE)." }

    $buildLog = Join-Path $OutputDir 'nmake-build.log'
    Write-Host "Building OpenSSL (log: $buildLog)..."
    & $nmake *> $buildLog
    if ($LASTEXITCODE -ne 0) { throw "OpenSSL nmake failed (exit $LASTEXITCODE). See $buildLog." }

    $testLog = Join-Path $OutputDir 'nmake-test.log'
    Write-Host "Running OpenSSL test suite (log: $testLog)..."
    & $nmake test *> $testLog
    if ($LASTEXITCODE -ne 0) { throw "OpenSSL nmake test failed (exit $LASTEXITCODE). See $testLog." }

    $installLog = Join-Path $OutputDir 'nmake-install.log'
    Write-Host "Installing static OpenSSL (log: $installLog)..."
    & $nmake install_sw *> $installLog
    if ($LASTEXITCODE -ne 0) { throw "OpenSSL nmake install_sw failed (exit $LASTEXITCODE). See $installLog." }
}
finally {
    Pop-Location
}

$opensslExe = Join-Path $installDir 'bin\openssl.exe'
foreach ($path in @($opensslExe, (Join-Path $installDir 'include\openssl\opensslv.h'), (Join-Path $installDir 'lib\libcrypto.lib'), (Join-Path $installDir 'lib\libssl.lib'))) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "OpenSSL installation is incomplete: $path" }
}
$versionText = (& $opensslExe version).Trim()
if ($LASTEXITCODE -ne 0 -or $versionText -notmatch '^OpenSSL 3\.5\.9\b') {
    throw "Unexpected installed OpenSSL version: $versionText"
}
Write-Host "Installed $versionText at $installDir"
& (Join-Path $PSScriptRoot 'write-openssl-manifest.ps1') -OpenSslDir $installDir -SourceArchive $archive
