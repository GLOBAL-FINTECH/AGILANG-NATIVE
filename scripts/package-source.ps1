param(
    [string]$Output = "agilang-native-runtime-source.zip"
)
$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$out = Join-Path $root $Output
if (Test-Path $out) { Remove-Item $out -Force }
$exclude = @('.git', '.venv', 'target', 'build')
$items = Get-ChildItem $root -Force | Where-Object { $exclude -notcontains $_.Name -and $_.FullName -ne $out }
Compress-Archive -Path $items.FullName -DestinationPath $out -CompressionLevel Optimal
Write-Host "Created clean source package: $out"
