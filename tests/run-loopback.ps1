param(
    [string]$Python = 'python',
    [string]$Cargo = 'cargo'
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$fixtureScript = Join-Path $PSScriptRoot 'ssh_fixture.py'
$pythonCommand = Get-Command $Python -CommandType Application -ErrorAction Stop | Select-Object -First 1
$cargoCommand = Get-Command $Cargo -CommandType Application -ErrorAction Stop | Select-Object -First 1

# Check prerequisites only. This runner never installs Python packages or tools.
& $pythonCommand.Source -c "import sys; assert sys.version_info >= (3, 10), 'Python 3.10+ required'; import paramiko; import cryptography"
if ($LASTEXITCODE -ne 0) {
    throw 'Python 3.10+, Paramiko et cryptography sont requis. Voir README.md. Utilisez -Python pour choisir votre environnement virtuel.'
}

$runDirectory = Join-Path $projectRoot ('test-work\loopback-' + [Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($runDirectory)
$statusPath = Join-Path $runDirectory 'ssh-fixture-status.json'
$statsPath = Join-Path $runDirectory 'ssh-fixture-stats.json'
$stopPath = Join-Path $runDirectory 'ssh-fixture-stop'
$variables = @('NEXUS_TEST_PORT', 'NEXUS_TEST_FINGERPRINT', 'NEXUS_TEST_STATS', 'NEXUS_TEST_PRIVATE_KEY')
$previous = @{}
foreach ($name in $variables) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$fixture = $null
$testExitCode = 1

try {
    # Windows paths cannot contain a quote; explicit quoting preserves spaces for Start-Process.
    $fixtureArguments = @('-u', ('"' + $fixtureScript + '"'), '--work-dir', ('"' + $runDirectory + '"'), '--port-file', ('"' + $statusPath + '"'))
    $fixture = Start-Process -FilePath $pythonCommand.Source -ArgumentList $fixtureArguments -WorkingDirectory $projectRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $runDirectory 'stdout.log') -RedirectStandardError (Join-Path $runDirectory 'stderr.log')
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    $state = $null
    while ([DateTime]::UtcNow -lt $deadline) {
        $fixture.Refresh()
        if ($fixture.HasExited) { throw ('La fixture SSH a quitté. Consultez ' + (Join-Path $runDirectory 'stderr.log')) }
        if (Test-Path -LiteralPath $statusPath) {
            try {
                $candidate = Get-Content -LiteralPath $statusPath -Raw | ConvertFrom-Json
                if ($candidate.pid -eq $fixture.Id -and $candidate.host -eq '127.0.0.1' -and $candidate.port -gt 1024 -and $candidate.port -le 65535 -and $candidate.fingerprint -match '^SHA256:[A-Za-z0-9+/]{43}$' -and (Test-Path -LiteralPath $statsPath) -and $candidate.private_key_path -and (Test-Path -LiteralPath $candidate.private_key_path)) {
                    $keyPath = [IO.Path]::GetFullPath([string]$candidate.private_key_path)
                    $allowedPrefix = [IO.Path]::GetFullPath($runDirectory).TrimEnd('\') + '\'
                    if (!$keyPath.StartsWith($allowedPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'La clé de test doit rester dans le dossier temporaire de cette exécution.' }
                    $state = $candidate
                    break
                }
            }
            catch [System.ArgumentException] { throw }
            catch { }
        }
        Start-Sleep -Milliseconds 200
    }
    if ($null -eq $state) { throw ('La fixture SSH locale ne répond pas après 60 secondes. Consultez ' + $runDirectory) }
    $env:NEXUS_TEST_PORT = [string]$state.port
    $env:NEXUS_TEST_FINGERPRINT = [string]$state.fingerprint
    $env:NEXUS_TEST_STATS = $statsPath
    $env:NEXUS_TEST_PRIVATE_KEY = [string]$state.private_key_path
    Write-Host ('Tests SSH/SFTP sur 127.0.0.1:' + $state.port + ' ; fichiers fictifs dans ' + $runDirectory)
    Push-Location -LiteralPath $projectRoot
    try {
        & $cargoCommand.Source test --release --locked -- --ignored --test-threads=1
        $testExitCode = $LASTEXITCODE
    }
    finally { Pop-Location }
}
finally {
    if ($null -ne $fixture) {
        [IO.File]::WriteAllText($stopPath, '')
        $stopDeadline = [DateTime]::UtcNow.AddSeconds(5)
        do {
            $fixture.Refresh()
            if ($fixture.HasExited) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $stopDeadline)
        if (!$fixture.HasExited) { Stop-Process -Id $fixture.Id -ErrorAction SilentlyContinue }
        $fixture.Dispose()
    }
    foreach ($name in $variables) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
}

if ($testExitCode -ne 0) { throw "Échec des tests d’intégration (code $testExitCode). Fichiers conservés : $runDirectory" }
Write-Host ('Tests terminés ; serveur local arrêté. Les journaux et données fictives sont conservés dans ' + $runDirectory)
