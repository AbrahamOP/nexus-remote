[CmdletBinding()]
param(
    [switch]$Test,
    [switch]$Offline
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$toolchain = 'stable-x86_64-pc-windows-msvc'

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
foreach ($tool in @('cl.exe', 'rc.exe', 'perl.exe')) {
    if (-not (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)) {
        throw "Missing $tool. Open an x64 Developer PowerShell with the C++ Build Tools, Windows SDK and Perl on PATH."
    }
}

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
