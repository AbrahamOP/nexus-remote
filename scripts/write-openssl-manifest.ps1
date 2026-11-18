[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$OpenSslDir,
    [string]$SourceArchive
)

$ErrorActionPreference = 'Stop'
$OpenSslDir = [IO.Path]::GetFullPath($OpenSslDir)
$stagingRoot = Split-Path -Path $OpenSslDir -Parent
if (-not $SourceArchive) { $SourceArchive = Join-Path $stagingRoot 'openssl-3.5.9.tar.gz' }
$SourceArchive = [IO.Path]::GetFullPath($SourceArchive)
$sourceSha256 = '603f5602e2eef00d77fbd429d34dcd5822bb301757a1bc9cdb24c670f1eb859a'

if (-not (Test-Path -LiteralPath $SourceArchive -PathType Leaf)) {
    throw "OpenSSL source archive missing: $SourceArchive"
}
if ((Get-FileHash -LiteralPath $SourceArchive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $sourceSha256) {
    throw 'OpenSSL 3.5.9 source archive SHA-256 does not match the official release.'
}

$relativeFiles = @('bin\openssl.exe', 'include\openssl\opensslv.h', 'lib\libcrypto.lib', 'lib\libssl.lib')
$hashes = [ordered]@{}
foreach ($relativeFile in $relativeFiles) {
    $path = Join-Path $OpenSslDir $relativeFile
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "OpenSSL installation is incomplete: $path" }
    $hashes[$relativeFile] = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
}
$opensslExe = Join-Path $OpenSslDir 'bin\openssl.exe'
$versionText = (& $opensslExe version).Trim()
if ($LASTEXITCODE -ne 0 -or $versionText -notmatch '^OpenSSL 3\.5\.9\b') {
    throw "Unexpected installed OpenSSL version: $versionText"
}
$manifest = [ordered]@{
    schema = 1
    openssl_version = '3.5.9'
    source_sha256 = $sourceSha256
    files = $hashes
}
$manifestPath = Join-Path $OpenSslDir 'nexus-openssl-manifest.json'
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $manifestPath -Encoding utf8
Write-Host "Verified OpenSSL 3.5.9 staging manifest: $manifestPath"
