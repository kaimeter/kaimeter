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
    $Output = Join-Path $paperDir ('dist/{0}-v{1}.pdf' -f $slug, $version)
}

$outputDir = Split-Path -Parent $Output
$stem = [System.IO.Path]::GetFileNameWithoutExtension($Output)
$texPath = Join-Path $outputDir "$stem.tex"
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

$year, $month = $date -split '-'
$epoch = [DateTimeOffset]::new([int]$year, [int]$month, 1, 0, 0, 0, [TimeSpan]::Zero).ToUnixTimeSeconds()
$env:SOURCE_DATE_EPOCH = $epoch.ToString([System.Globalization.CultureInfo]::InvariantCulture)
$env:FORCE_SOURCE_DATE = '1'

# Deriving the PDF trailer ID from the inputs keeps repeated builds identical.
$inputBytes = [System.Collections.Generic.List[byte]]::new()
foreach ($path in @($sourcePath, $metadataPath, $templatePath, $filterPath)) {
    $inputBytes.AddRange([System.IO.File]::ReadAllBytes($path))
}
$digest = [System.Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($inputBytes.ToArray())).ToLowerInvariant()
$trailerId = '<{0}> <{1}>' -f $digest.Substring(0, 32), $digest.Substring(32, 32)

$pandocArgs = @(
    $sourcePath
    '--from', 'markdown'
    '--metadata-file', $metadataPath
    '--template', $templatePath
    '--lua-filter', $filterPath
    '--variable', "pdf-trailer-id=$trailerId"
    '--to', 'latex'
    '--standalone'
    '--output', $texPath
)

& pandoc @pandocArgs
if ($LASTEXITCODE -ne 0) {
    throw "pandoc exited with code $LASTEXITCODE"
}

# Two passes resolve the internal links; stable file names keep the font
# subset tags reproducible.
$latexArgs = @(
    '-halt-on-error'
    '-interaction=nonstopmode'
    '-file-line-error'
    "-output-directory=$outputDir"
    $texPath
)
foreach ($pass in 1..2) {
    $latexOutput = & xelatex @latexArgs 2>&1
    if ($LASTEXITCODE -ne 0) {
        $latexOutput | Write-Host
        throw "xelatex exited with code $LASTEXITCODE on pass $pass"
    }
}

foreach ($extension in @('aux', 'log', 'out', 'toc')) {
    Remove-Item -LiteralPath (Join-Path $outputDir "$stem.$extension") -Force -ErrorAction SilentlyContinue
}

$hash = (Get-FileHash -LiteralPath $Output -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Host "Built $Output"
Write-Host "SHA-256 $hash"
