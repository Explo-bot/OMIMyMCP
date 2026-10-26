# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0

<#
.SYNOPSIS
    Prova di installazione del servizio Windows (da eseguire come amministratore).

.DESCRIPTION
    Installa omi-mcp come servizio di prova (OMIMyMCP-Test, porta 18765, con token e regola del firewall)
    in C:\ProgramData\OMIMyMCP-Test, perché l'account LocalService non può leggere i file nel profilo
    utente. Verifica: stato Running, service status, /health, 401 senza token, tools/list (9 tool) con
    Invoke-RestMethod, righe del ciclo di vita nel log senza il token, stop, start, uninstall, servizio
    sparito e regola del firewall rimossa. Alla fine elimina tutto ed esce con 0 solo se ogni verifica è
    riuscita.

    Senza -Db usa il database sintetico dei test, generato con
    cargo test -p omi-core --test service -- --ignored genera_database_di_prova.

.PARAMETER SourceDir
    Cartella con omi-mcp.exe. Default: target\release del repository.
.PARAMETER Db
    Database da usare (viene copiato). Default: quello sintetico dei test.
.PARAMETER ServiceName
    Nome del servizio di prova. Default OMIMyMCP-Test.
.PARAMETER Port
    Porta del servizio di prova. Default 18765.
.PARAMETER KeepFiles
    Non elimina C:\ProgramData\OMIMyMCP-Test alla fine (utile per leggere il log).
#>
[CmdletBinding()]
param(
    [string]$SourceDir = (Join-Path $PSScriptRoot '..\..\target\release'),
    [string]$Db,
    [string]$ServiceName = 'OMIMyMCP-Test',
    [int]$Port = 18765,
    [switch]$KeepFiles
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$p = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error 'Servono i privilegi di amministratore: aprire PowerShell con "Esegui come amministratore".'
    exit 1
}

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$sorgente = Join-Path $SourceDir 'omi-mcp.exe'
if (-not (Test-Path $sorgente)) { throw "omi-mcp.exe non trovato in $SourceDir (cargo build --release)." }

$base = Join-Path $env:ProgramData 'OMIMyMCP-Test'
$bin = Join-Path $base 'bin'
$dati = Join-Path $base 'dati'
$exe = Join-Path $bin 'omi-mcp.exe'
$dbTest = Join-Path $dati 'omi.db'
$tokenFile = Join-Path $dati 'token.txt'
$url = "http://127.0.0.1:$Port"

$script:falliti = 0
function Test-Passo([string]$Descrizione, [scriptblock]$Verifica) {
    try {
        $esito = & $Verifica
        if ($esito -eq $false) { throw 'condizione non verificata' }
        Write-Host "[OK]      $Descrizione" -ForegroundColor Green
    } catch {
        $script:falliti++
        Write-Host "[FALLITO] $Descrizione - $($_.Exception.Message)" -ForegroundColor Red
    }
}

function Invoke-OmiMcp([string[]]$Argomenti) {
    Write-Host "> omi-mcp $($Argomenti -join ' ')"
    & $exe @Argomenti | ForEach-Object { Write-Host "  $_" }
    $LASTEXITCODE
}

function Get-Stato { (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Status)" }) }

function Get-RegolaFirewall { Get-NetFirewallRule -DisplayName $ServiceName -ErrorAction SilentlyContinue }

function Invoke-Mcp([hashtable]$Intestazioni) {
    Invoke-RestMethod -Method Post -Uri "$url/mcp" -ContentType 'application/json' -Headers $Intestazioni `
        -Body '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
}

# Preparazione: residui di una prova precedente, binario, database e token.
if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
    Write-Host "==> Residuo di una prova precedente: disinstallo $ServiceName"
    $vecchio = if (Test-Path $exe) { $exe } else { $sorgente }
    & $vecchio service uninstall --name $ServiceName | Out-Null
}
if (Test-Path $base) { Remove-Item -Recurse -Force $base }
New-Item -ItemType Directory -Force -Path $bin, $dati | Out-Null
Copy-Item $sorgente $exe

if ($Db) {
    Copy-Item (Resolve-Path $Db).Path $dbTest
} else {
    Write-Host '==> Generazione del database sintetico dei test'
    $fixture = Join-Path ([IO.Path]::GetTempPath()) "omimymcp-fixture-$PID"
    $env:OMI_FIXTURE_DIR = $fixture
    try {
        Push-Location $repo
        try {
            & cargo test -q -p omi-core --test service -- --ignored genera_database_di_prova | Out-Null
            if ($LASTEXITCODE -ne 0) { throw "cargo test è terminato con codice $LASTEXITCODE." }
        } finally { Pop-Location }
        Copy-Item (Join-Path $fixture 'omi.db') $dbTest
    } finally {
        Remove-Item Env:\OMI_FIXTURE_DIR
        Remove-Item -Recurse -Force $fixture -ErrorAction SilentlyContinue
    }
}
$bytes = New-Object byte[] 32
$rng = [Security.Cryptography.RandomNumberGenerator]::Create()
try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }
$token = [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')
[IO.File]::WriteAllText($tokenFile, $token)
$conToken = @{ Accept = 'application/json, text/event-stream'; Authorization = "Bearer $token" }

try {
    Write-Host "==> Installazione di $ServiceName sulla porta $Port"
    Test-Passo 'service install --firewall --now' {
        (Invoke-OmiMcp @('service', 'install', '--name', $ServiceName, '--db', $dbTest, '--port', "$Port",
                '--token-file', $tokenFile, '--firewall', '--now')) -eq 0
    }
    Test-Passo 'stato Running' { (Get-Stato) -eq 'Running' }
    Test-Passo 'regola del firewall presente' { $null -ne (Get-RegolaFirewall) }
    Test-Passo 'service status' { (Invoke-OmiMcp @('service', 'status', '--name', $ServiceName)) -eq 0 }
    Test-Passo 'GET /health -> 200 ok' {
        $r = Invoke-WebRequest -Uri "$url/health" -UseBasicParsing
        $r.StatusCode -eq 200 -and $r.Content -eq 'ok'
    }
    Test-Passo 'POST /mcp senza token -> 401' {
        try { Invoke-Mcp @{ Accept = 'application/json, text/event-stream' } | Out-Null; $false }
        catch { [int]$_.Exception.Response.StatusCode -eq 401 }
    }
    Test-Passo 'tools/list -> 9 tool' {
        $r = Invoke-Mcp $conToken
        @($r.result.tools).Count -eq 9
    }

    Write-Host '==> Arresto e riavvio'
    Test-Passo 'service stop' { (Invoke-OmiMcp @('service', 'stop', '--name', $ServiceName)) -eq 0 }
    Test-Passo 'stato Stopped' { (Get-Stato) -eq 'Stopped' }
    Test-Passo 'service start' { (Invoke-OmiMcp @('service', 'start', '--name', $ServiceName)) -eq 0 }
    Test-Passo 'stato Running dopo il riavvio' { (Get-Stato) -eq 'Running' }
    Test-Passo 'tools/list dopo il riavvio' { @((Invoke-Mcp $conToken).result.tools).Count -eq 9 }

    Write-Host '==> Disinstallazione'
    Test-Passo 'service uninstall' { (Invoke-OmiMcp @('service', 'uninstall', '--name', $ServiceName)) -eq 0 }
    Test-Passo 'servizio non più presente' { $null -eq (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) }
    Test-Passo 'regola del firewall rimossa' { $null -eq (Get-RegolaFirewall) }
    Test-Passo 'database lasciato al suo posto' { Test-Path $dbTest }
    Test-Passo 'log: avvio, ascolto e arresto, senza il token' {
        $log = Get-Content -Raw (Join-Path $dati 'omi.log')
        ($log -match 'Avvio del servizio') -and ($log -match 'Server MCP in ascolto') -and
        ($log -match 'arrestato') -and -not $log.Contains($token)
    }
    Test-Passo 'service status dopo la disinstallazione -> non installato' {
        (Invoke-OmiMcp @('service', 'status', '--name', $ServiceName)) -eq 1
    }
} finally {
    if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
        & $exe service uninstall --name $ServiceName | Out-Null
    }
    if (-not $KeepFiles) { Remove-Item -Recurse -Force $base -ErrorAction SilentlyContinue }
}

if ($script:falliti -eq 0) {
    Write-Host 'Prova del servizio: tutte le verifiche sono riuscite.' -ForegroundColor Green
    exit 0
}
Write-Host "Prova del servizio: $script:falliti verifiche fallite." -ForegroundColor Red
exit 1
