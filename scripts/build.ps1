[CmdletBinding()]
param(
    [switch]$Test,
    [switch]$Offline,
    [string]$OpenSslDir
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$toolchain = 'stable-x86_64-pc-windows-msvc'
if (-not $OpenSslDir) { $OpenSslDir = $env:OPENSSL_DIR }
if (-not $OpenSslDir) { $OpenSslDir = Join-Path $projectRoot '.local-build\openssl-3.5.9\install' }
$OpenSslDir = [IO.Path]::GetFullPath($OpenSslDir)

if ($env:OS -ne 'Windows_NT') {
    throw 'This build script requires Windows x64.'
}
if ($env:VSCMD_ARG_TGT_ARCH -and $env:VSCMD_ARG_TGT_ARCH -ne 'x64') {
    throw 'Open an x64 Developer PowerShell for Visual Studio and run this script again.'
}

$rustupCommand = Get-Command rustup.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $rustupCommand) {
    throw 'Install Rust stable for x86_64-pc-windows-msvc with rustup before building.'
}
foreach ($tool in @('cl.exe', 'rc.exe')) {
    if (-not (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)) {
        throw "Missing $tool. Open an x64 Developer PowerShell with the C++ Build Tools and Windows SDK on PATH."
    }
}

# The Rust release must link to the separately verified, supported LTS build.
# Do not fall back to openssl-src's bundled 3.6.3 (end of support).
$opensslExe = Join-Path $OpenSslDir 'bin\openssl.exe'
$opensslHeader = Join-Path $OpenSslDir 'include\openssl\opensslv.h'
foreach ($path in @($opensslExe, $opensslHeader, (Join-Path $OpenSslDir 'lib\libcrypto.lib'), (Join-Path $OpenSslDir 'lib\libssl.lib'))) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "OpenSSL 3.5.9 LTS staging missing: $path. Run scripts/build-openssl.ps1 first."
    }
}
$opensslVersion = (& $opensslExe version).Trim()
if ($LASTEXITCODE -ne 0 -or $opensslVersion -notmatch '^OpenSSL 3\.5\.9\b') {
    throw "Expected OpenSSL 3.5.9 LTS; found: $opensslVersion"
}
$headerText = Get-Content -LiteralPath $opensslHeader -Raw
if ($headerText -notmatch '(?m)^\s*#\s*define\s+OPENSSL_VERSION_MAJOR\s+3\s*$' -or
    $headerText -notmatch '(?m)^\s*#\s*define\s+OPENSSL_VERSION_MINOR\s+5\s*$' -or
    $headerText -notmatch '(?m)^\s*#\s*define\s+OPENSSL_VERSION_PATCH\s+9\s*$') {
    throw "OpenSSL headers do not match 3.5.9: $opensslHeader"
}
$sourceSha256 = '603f5602e2eef00d77fbd429d34dcd5822bb301757a1bc9cdb24c670f1eb859a'
$sourceArchive = Join-Path (Split-Path -Path $OpenSslDir -Parent) 'openssl-3.5.9.tar.gz'
if (-not (Test-Path -LiteralPath $sourceArchive -PathType Leaf) -or
    (Get-FileHash -LiteralPath $sourceArchive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $sourceSha256) {
    throw "Official OpenSSL 3.5.9 source archive is missing or has changed: $sourceArchive"
}
$manifestPath = Join-Path $OpenSslDir 'nexus-openssl-manifest.json'
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "OpenSSL staging manifest missing: $manifestPath. Run scripts/build-openssl.ps1 first."
}
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.schema -ne 1 -or $manifest.openssl_version -ne '3.5.9' -or $manifest.source_sha256 -ne $sourceSha256) {
    throw "OpenSSL staging manifest is invalid: $manifestPath"
}
foreach ($relativeFile in @('bin\openssl.exe', 'include\openssl\opensslv.h', 'lib\libcrypto.lib', 'lib\libssl.lib')) {
    if (-not $manifest.files) { throw "OpenSSL staging manifest has no file hashes: $manifestPath" }
    $entry = $manifest.files.PSObject.Properties[$relativeFile]
    if (-not $entry) { throw "OpenSSL staging manifest has no hash for $relativeFile" }
    $expected = $entry.Value
    $actual = (Get-FileHash -LiteralPath (Join-Path $OpenSslDir $relativeFile) -Algorithm SHA256).Hash.ToLowerInvariant()
    if (-not $expected -or $actual -ne $expected) {
        throw "OpenSSL staging file changed since validation: $relativeFile"
    }
}
$env:OPENSSL_DIR = $OpenSslDir
$env:OPENSSL_STATIC = '1'
$env:OPENSSL_NO_VENDOR = '1'
Write-Host "Linking Nexus Remote with $opensslVersion from $OpenSslDir"

# Inspect installed toolchains only; never install or update development tools.
$installedToolchains = @(& $rustupCommand.Source toolchain list)
if ($LASTEXITCODE -ne 0 -or -not ($installedToolchains -match ('^' + [regex]::Escape($toolchain) + '(\s|$)'))) {
    throw "The installed Rust toolchain $toolchain is required. Install it before running this script."
}

Push-Location -LiteralPath $projectRoot
try {
    $cargoArguments = @('build', '--release', '--locked')
    if ($Offline) { $cargoArguments += '--offline' }
    & $rustupCommand.Source run $toolchain cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) { throw "Cargo build failed (exit $LASTEXITCODE)." }

    $cargoArguments = @('test', '--release', '--locked', '--test', 'openssl_link')
    if ($Offline) { $cargoArguments += '--offline' }
    & $rustupCommand.Source run $toolchain cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) { throw "Linked OpenSSL version test failed (exit $LASTEXITCODE)." }

    if ($Test) {
        $cargoArguments = @('test', '--release', '--locked')
        if ($Offline) { $cargoArguments += '--offline' }
        & $rustupCommand.Source run $toolchain cargo @cargoArguments
        if ($LASTEXITCODE -ne 0) { throw "Cargo test failed (exit $LASTEXITCODE)." }
    }
}
finally {
    Pop-Location
}
