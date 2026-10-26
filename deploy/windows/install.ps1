# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0

<#
.SYNOPSIS
    Installa OMIMyMCP come servizio Windows.

.DESCRIPTION
    1. Copia omi-mcp.exe, omi-import.exe, LICENSE, NOTICE e MANUALE_TECNICO.md in InstallDir.
    2. Crea DataDir e, con -WithToken, genera un token casuale in DataDir\token.txt (se manca), leggibile
       solo da Administrators e dall'account del servizio (LocalService).
    3. Se DataDir\omi.db non esiste si ferma: il database va prima caricato con omi-import.
    4. Installa e avvia il servizio con "omi-mcp service install ... --now".
    5. Con -AddToPath aggiunge InstallDir al PATH di sistema.

    Lo script è idempotente: se il servizio esiste già lo disinstalla (lasciando la regola del firewall,
    il database, il log e il token) e lo reinstalla con i nuovi binari. Va eseguito come amministratore.

.PARAMETER SourceDir
    Cartella con omi-mcp.exe e omi-import.exe (ad esempio target\release o il pacchetto di rilascio).
.PARAMETER InstallDir
    Cartella dei programmi. Default: C:\Program Files\OMIMyMCP.
.PARAMETER DataDir
    Cartella dei dati (omi.db, omi.log, token.txt). Default: C:\ProgramData\OMIMyMCP.
.PARAMETER Port
    Porta TCP del server. Default 8765.
.PARAMETER Bind
    Indirizzo di ascolto. Default 127.0.0.1 (solo locale, oppure dietro Caddy).
.PARAMETER ServiceName
    Nome del servizio. Default OMIMyMCP.
.PARAMETER Log
    Registra anche le richieste HTTP e le chiamate ai tool in DataDir\omi.log.
.PARAMETER Firewall
    Aggiunge la regola in ingresso nel firewall di Windows.
.PARAMETER WithToken
    Richiede il token Bearer su /mcp (file DataDir\token.txt).
.PARAMETER TrustProxy
    Nel log usa l'IP del client indicato da X-Forwarded-For (--trust-proxy). Da usare dietro Caddy
    (install-caddy.ps1), con -Bind 127.0.0.1 e senza -Firewall.
.PARAMETER AddToPath
    Aggiunge InstallDir al PATH di sistema.

.EXAMPLE
    .\install.ps1 -SourceDir ..\..\target\release -WithToken

.EXAMPLE
    .\install.ps1 -SourceDir ..\..\target\release -TrustProxy
#>
[CmdletBinding(SupportsShouldProcess)]
param(
    [Parameter(Mandatory)][string]$SourceDir,
    [string]$InstallDir = (Join-Path $env:ProgramFiles 'OMIMyMCP'),
    [string]$DataDir = (Join-Path $env:ProgramData 'OMIMyMCP'),
    [ValidateRange(1, 65535)][int]$Port = 8765,
    [string]$Bind = '127.0.0.1',
    [string]$ServiceName = 'OMIMyMCP',
    [switch]$Log,
    [switch]$Firewall,
    [switch]$WithToken,
    [switch]$TrustProxy,
    [switch]$AddToPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Test-Amministratore {
    $p = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    $p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Invoke-Nativo([string]$Exe, [string[]]$Argomenti) {
    Write-Host "> $Exe $($Argomenti -join ' ')"
    & $Exe @Argomenti
    if ($LASTEXITCODE -ne 0) { throw "$Exe è terminato con codice $LASTEXITCODE." }
}

if (-not $WhatIfPreference -and -not (Test-Amministratore)) {
    Write-Error 'Servono i privilegi di amministratore: aprire PowerShell con "Esegui come amministratore".'
    exit 1
}

$SourceDir = (Resolve-Path $SourceDir).Path
$repo = Resolve-Path (Join-Path $PSScriptRoot '..\..')
foreach ($exe in 'omi-mcp.exe', 'omi-import.exe') {
    if (-not (Test-Path (Join-Path $SourceDir $exe))) { throw "File mancante: $(Join-Path $SourceDir $exe)" }
}

# 0. Un servizio già installato blocca la sovrascrittura degli eseguibili: lo si disinstalla prima.
if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
    $esistente = Join-Path $InstallDir 'omi-mcp.exe'
    if (-not (Test-Path $esistente)) { $esistente = Join-Path $SourceDir 'omi-mcp.exe' }
    if ($PSCmdlet.ShouldProcess($ServiceName, 'Disinstallazione del servizio esistente')) {
        Write-Host "==> Il servizio $ServiceName esiste già: lo disinstallo prima di aggiornare i file."
        Invoke-Nativo $esistente @('service', 'uninstall', '--name', $ServiceName, '--keep-firewall')
    }
}

# 1. Programmi e documenti.
Write-Host "==> Copia dei programmi in $InstallDir"
if ($PSCmdlet.ShouldProcess($InstallDir, 'Creazione della cartella')) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}
$file = [ordered]@{
    'omi-mcp.exe'        = @(Join-Path $SourceDir 'omi-mcp.exe')
    'omi-import.exe'     = @(Join-Path $SourceDir 'omi-import.exe')
    'LICENSE'            = @((Join-Path $SourceDir 'LICENSE'), (Join-Path $repo 'LICENSE'))
    'NOTICE'             = @((Join-Path $SourceDir 'NOTICE'), (Join-Path $repo 'NOTICE'))
    'MANUALE_TECNICO.md' = @((Join-Path $SourceDir 'MANUALE_TECNICO.md'), (Join-Path $repo 'docs\MANUALE_TECNICO.md'))
}
foreach ($nome in $file.Keys) {
    $origine = $file[$nome] | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $origine) {
        Write-Warning "$nome non trovato in $SourceDir né nel repository: non copiato."
        continue
    }
    if ($PSCmdlet.ShouldProcess((Join-Path $InstallDir $nome), "Copia da $origine")) {
        Copy-Item -Path $origine -Destination (Join-Path $InstallDir $nome) -Force
        Write-Host "    $nome"
    }
}

# 2. Cartella dei dati e token.
Write-Host "==> Cartella dei dati $DataDir"
if ($PSCmdlet.ShouldProcess($DataDir, 'Creazione della cartella')) {
    New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
}
$tokenFile = Join-Path $DataDir 'token.txt'
if ($WithToken) {
    if (Test-Path $tokenFile) {
        Write-Host "    Token già presente: $tokenFile (non modificato)"
    } elseif ($PSCmdlet.ShouldProcess($tokenFile, 'Generazione del token')) {
        $bytes = New-Object byte[] 32
        $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
        try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }
        $token = [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')
        [IO.File]::WriteAllText($tokenFile, $token)
        Write-Host "    Token generato: $tokenFile"
    }
    if ($PSCmdlet.ShouldProcess($tokenFile, 'ACL: solo Administrators e LocalService')) {
        # S-1-5-32-544 = Administrators, S-1-5-19 = LocalService (account del servizio).
        Invoke-Nativo 'icacls.exe' @($tokenFile, '/inheritance:r', '/grant:r', '*S-1-5-32-544:F', '*S-1-5-19:R')
    }
}

# 3. Il database va caricato prima con omi-import.
$db = Join-Path $DataDir 'omi.db'
if (-not (Test-Path $db)) {
    Write-Warning "Database non trovato: $db. Caricarlo prima con omi-import, ad esempio:"
    Write-Warning "  & '$(Join-Path $InstallDir 'omi-import.exe')' '$db' 'C:\cartella\QI*.zip'"
    Write-Warning 'Poi rilanciare questo script per installare il servizio.'
    exit 1
}

# 4. Servizio.
$argomenti = @('service', 'install', '--name', $ServiceName, '--db', $db, '--port', "$Port", '--bind', $Bind)
if ($Log) { $argomenti += '--log' }
if ($Firewall) { $argomenti += '--firewall' }
if ($TrustProxy) { $argomenti += '--trust-proxy' }
if ($WithToken) { $argomenti += @('--token-file', $tokenFile) }
$argomenti += '--now'
Write-Host "==> Installazione del servizio $ServiceName"
if ($PSCmdlet.ShouldProcess($ServiceName, 'Installazione e avvio del servizio')) {
    Invoke-Nativo (Join-Path $InstallDir 'omi-mcp.exe') $argomenti
}

# 5. PATH di sistema (facoltativo).
if ($AddToPath) {
    $path = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $voci = $path -split ';' | Where-Object { $_ -ne '' }
    if ($voci -contains $InstallDir) {
        Write-Host "==> $InstallDir è già nel PATH di sistema."
    } elseif ($PSCmdlet.ShouldProcess('PATH di sistema', "Aggiunta di $InstallDir")) {
        [Environment]::SetEnvironmentVariable('Path', (($voci + $InstallDir) -join ';'), 'Machine')
        Write-Host "==> $InstallDir aggiunto al PATH di sistema (vale per le nuove finestre)."
    }
}

if ($WithToken) {
    Write-Host "Il client MCP deve inviare 'Authorization: Bearer <token>' con il contenuto di $tokenFile."
}
Write-Host 'Installazione completata.'
