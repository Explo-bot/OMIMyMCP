# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0

<#
.SYNOPSIS
    Crea il pacchetto di rilascio per Windows x64.

.DESCRIPTION
    1. Esegue "cargo test --workspace" (salvo -SkipTests).
    2. Compila con "cargo build --release --workspace --target x86_64-pc-windows-msvc".
    3. Crea dist\OMIMyMCP-rs-<versione>-win-x64\ con omi-import.exe, omi-mcp.exe, LICENSE, NOTICE,
       MANUALE_TECNICO.md (e MANUALE_TECNICO.html, se esiste in docs\), LEGGIMI.md, deploy\windows\*.ps1 e
       deploy\caddy\*.example (con LEGGIMI.md, a cui rimanda il manuale).
    4. Con -CertThumbprint firma i due eseguibili con signtool (Windows SDK).
    5. Comprime la cartella in dist\OMIMyMCP-rs-<versione>-win-x64.zip e scrive dist\SHA256SUMS.txt.

    La versione è quella di [workspace.package] in Cargo.toml. Non servono privilegi di amministratore.

.PARAMETER CertThumbprint
    Identificazione personale (SHA-1) di un certificato di firma del codice presente nell'archivio dei
    certificati: se indicata, gli eseguibili vengono firmati (richiede signtool.exe).
.PARAMETER TimestampUrl
    Server di marcatura temporale usato con -CertThumbprint. Default: http://timestamp.digicert.com.
.PARAMETER SkipTests
    Salta "cargo test --workspace" (da usare solo se i test sono già stati eseguiti).
.PARAMETER OutDir
    Cartella di destinazione. Default: dist nella radice del progetto.

.EXAMPLE
    .\deploy\windows\build-release.ps1

.EXAMPLE
    .\deploy\windows\build-release.ps1 -CertThumbprint 0123456789ABCDEF0123456789ABCDEF01234567
#>
[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$CertThumbprint,
    [string]$TimestampUrl = 'http://timestamp.digicert.com',
    [switch]$SkipTests,
    [string]$OutDir
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$target = 'x86_64-pc-windows-msvc'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $OutDir) { $OutDir = Join-Path $repo 'dist' }

function Invoke-Nativo([string]$Exe, [string[]]$Argomenti) {
    Write-Host "> $Exe $($Argomenti -join ' ')"
    & $Exe @Argomenti
    if ($LASTEXITCODE -ne 0) { throw "$Exe è terminato con codice $LASTEXITCODE." }
}

# La versione è la prima riga "version = ..." dopo [workspace.package].
$cargoToml = Get-Content -Raw -Path (Join-Path $repo 'Cargo.toml')
if ($cargoToml -notmatch '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"') {
    throw 'Versione non trovata in [workspace.package] di Cargo.toml.'
}
$versione = $Matches[1]
$nome = "OMIMyMCP-rs-$versione-win-x64"
$stage = Join-Path $OutDir $nome
$zip = Join-Path $OutDir "$nome.zip"
Write-Host "==> Pacchetto $nome"

Push-Location $repo
try {
    # 1. Test.
    if ($SkipTests) {
        Write-Warning 'Test saltati (-SkipTests).'
    } elseif ($PSCmdlet.ShouldProcess('workspace', 'cargo test')) {
        Write-Host '==> Test'
        Invoke-Nativo 'cargo' @('test', '--workspace')
    }

    # 2. Compilazione.
    if ($PSCmdlet.ShouldProcess('workspace', "cargo build --release --target $target")) {
        Write-Host '==> Compilazione'
        Invoke-Nativo 'cargo' @('build', '--release', '--workspace', '--target', $target)
    }
    $bin = Join-Path $repo "target\$target\release"

    # 3. Cartella del pacchetto.
    Write-Host "==> Cartella $stage"
    if ($PSCmdlet.ShouldProcess($stage, 'Creazione della cartella del pacchetto')) {
        if (Test-Path $stage) { Remove-Item -Recurse -Force -Path $stage }
        New-Item -ItemType Directory -Force -Path (Join-Path $stage 'deploy\windows'), (Join-Path $stage 'deploy\caddy') | Out-Null

        foreach ($exe in 'omi-import.exe', 'omi-mcp.exe') {
            if (-not (Test-Path (Join-Path $bin $exe))) { throw "File mancante: $(Join-Path $bin $exe)" }
            Copy-Item -Path (Join-Path $bin $exe) -Destination $stage
        }
        foreach ($f in 'LICENSE', 'NOTICE') {
            Copy-Item -Path (Join-Path $repo $f) -Destination $stage
        }
        Copy-Item -Path (Join-Path $repo 'docs\MANUALE_TECNICO.md') -Destination $stage
        Copy-Item -Path (Join-Path $repo 'deploy\windows\LEGGIMI.md') -Destination $stage
        $html = Join-Path $repo 'docs\MANUALE_TECNICO.html'
        if (Test-Path $html) { Copy-Item -Path $html -Destination $stage }
        Copy-Item -Path (Join-Path $repo 'deploy\windows\*.ps1') -Destination (Join-Path $stage 'deploy\windows')
        Copy-Item -Path (Join-Path $repo 'deploy\caddy\*.example'), (Join-Path $repo 'deploy\caddy\LEGGIMI.md') `
            -Destination (Join-Path $stage 'deploy\caddy')
    }

    # 4. Firma (facoltativa).
    if ($CertThumbprint) {
        $signtool = (Get-Command 'signtool.exe' -ErrorAction SilentlyContinue).Source
        if (-not $signtool) {
            $kit = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
            if (Test-Path $kit) {
                $signtool = Get-ChildItem -Path $kit -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
                    Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending |
                    Select-Object -First 1 -ExpandProperty FullName
            }
        }
        if (-not $signtool) { throw 'signtool.exe non trovato: installare il Windows SDK oppure togliere -CertThumbprint.' }
        foreach ($exe in 'omi-import.exe', 'omi-mcp.exe') {
            if ($PSCmdlet.ShouldProcess((Join-Path $stage $exe), 'Firma con signtool')) {
                Invoke-Nativo $signtool @('sign', '/sha1', $CertThumbprint, '/fd', 'SHA256', '/tr', $TimestampUrl, '/td', 'SHA256', (Join-Path $stage $exe))
            }
        }
    }

    # 5. ZIP e SHA256SUMS.txt (l'hash è calcolato con .NET: Get-FileHash di Windows PowerShell 5.1 rispetta -WhatIf).
    Write-Host "==> $zip"
    if ($PSCmdlet.ShouldProcess($zip, 'Creazione dello ZIP e di SHA256SUMS.txt')) {
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        if (Test-Path $zip) { Remove-Item -Force -Path $zip }
        [IO.Compression.ZipFile]::CreateFromDirectory($stage, $zip, [IO.Compression.CompressionLevel]::Optimal, $true)

        $sha = [Security.Cryptography.SHA256]::Create()
        $stream = [IO.File]::OpenRead($zip)
        try { $hash = ($sha.ComputeHash($stream) | ForEach-Object { $_.ToString('x2') }) -join '' }
        finally { $stream.Dispose(); $sha.Dispose() }
        [IO.File]::WriteAllText((Join-Path $OutDir 'SHA256SUMS.txt'), "$hash  $nome.zip`n", (New-Object Text.UTF8Encoding($false)))
        Write-Host "    SHA-256: $hash"
        Write-Host "    $((Get-Item $zip).Length) byte"
    }
} finally {
    Pop-Location
}
Write-Host 'Pacchetto completato.'
