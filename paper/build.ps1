#!/usr/bin/env pwsh
# Builds the working-paper PDF with Pandoc and XeLaTeX.
#
# The PDF is a release artifact: it lands in paper/dist and is never tracked
# by git. SOURCE_DATE_EPOCH and FORCE_SOURCE_DATE pin the embedded timestamp
# so a fixed toolchain produces a byte-stable document.

[CmdletBinding()]
param(
    # Override the output path; defaults to the release artifact name.
    [string]$Output
)

$ErrorActionPreference = 'Stop'

$paperDir = $PSScriptRoot
$sourcePath = Join-Path $paperDir 'whitepaper.md'
$metadataPath = Join-Path $paperDir 'metadata.yaml'
$templatePath = Join-Path $paperDir 'templates/paper.tex'
$filterPath = Join-Path $paperDir 'filters/paper.lua'

foreach ($command in @('pandoc', 'xelatex')) {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
        throw "$command is required to build the PDF but was not found on PATH"
    }
}

$metadata = Get-Content -LiteralPath $metadataPath -Raw

function Get-MetadataField([string]$name) {
    $pattern = "(?m)^${name}:\s*`"?(.+?)`"?\s*$"
    $match = [regex]::Match($metadata, $pattern)
    if (-not $match.Success) {
        throw "metadata.yaml does not define $name"
    }
    return $match.Groups[1].Value
}

$series = Get-MetadataField 'series'
$version = Get-MetadataField 'version'
$date = Get-MetadataField 'date'

if (-not $Output) {
    $slug = ($series.ToLowerInvariant() -replace '[^a-z0-9]+', '-').Trim('-')
    $fileName = '{0}-v{1}.pdf' -f $slug, $version
    $Output = Join-Path $paperDir "dist/$fileName"
}

$outputDir = Split-Path -Parent $Output
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

$year, $month = $date -split '-'
$epoch = [DateTimeOffset]::new([int]$year, [int]$month, 1, 0, 0, 0, [TimeSpan]::Zero).ToUnixTimeSeconds()
$env:SOURCE_DATE_EPOCH = $epoch.ToString([System.Globalization.CultureInfo]::InvariantCulture)
$env:FORCE_SOURCE_DATE = '1'

$pandocArgs = @(
    $sourcePath
    '--from', 'markdown'
    '--metadata-file', $metadataPath
    '--template', $templatePath
    '--lua-filter', $filterPath
    '--pdf-engine', 'xelatex'
    '--output', $Output
)

& pandoc @pandocArgs
if ($LASTEXITCODE -ne 0) {
    throw "pandoc exited with code $LASTEXITCODE"
}

$hash = (Get-FileHash -LiteralPath $Output -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Host "Built $Output"
Write-Host "SHA-256 $hash"
