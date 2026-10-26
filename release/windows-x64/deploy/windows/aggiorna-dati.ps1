# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0

<#
.SYNOPSIS
    Carica nuove forniture OMI nel database del servizio.

.DESCRIPTION
    Ferma il servizio, esegue omi-import sui file QI*.zip / QIP*.zip di ZipDir, riavvia il servizio (anche
    se l'import non riesce) e mostra il riepilogo stampato da omi-import. Il server può leggere durante
    l'import, ma fermarlo evita blocchi sui caricamenti nazionali di grandi dimensioni.

    I nomi dei file non vengono stampati dallo script perché contengono il codice fiscale del richiedente.
    Codice d'uscita: quello di omi-import (0 tutto caricato, 2 almeno un file con errori).

.PARAMETER ZipDir
    Cartella con i file QI*.zip scaricati dal sito dell'Agenzia delle Entrate.
.PARAMETER DataDir
    Cartella dei dati. Default: C:\ProgramData\OMIMyMCP.
.PARAMETER InstallDir
    Cartella dei programmi. Default: C:\Program Files\OMIMyMCP.
.PARAMETER ServiceName
    Nome del servizio. Default OMIMyMCP.
#>
[CmdletBinding(SupportsShouldProcess)]
param(
    [Parameter(Mandatory)][string]$ZipDir,
    [string]$DataDir = (Join-Path $env:ProgramData 'OMIMyMCP'),
    [string]$InstallDir = (Join-Path $env:ProgramFiles 'OMIMyMCP'),
    [string]$ServiceName = 'OMIMyMCP'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$p = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $WhatIfPreference -and -not $p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error 'Servono i privilegi di amministratore: aprire PowerShell con "Esegui come amministratore".'
    exit 1
}

$import = Join-Path $InstallDir 'omi-import.exe'
if (-not (Test-Path $import)) { throw "omi-import.exe non trovato in $InstallDir." }
$db = Join-Path $DataDir 'omi.db'
$zip = @(Get-ChildItem -Path $ZipDir -Filter 'QI*.zip' -File | Sort-Object Name)
if ($zip.Count -eq 0) { throw "Nessun file QI*.zip in $ZipDir." }
Write-Host "==> $($zip.Count) file QI*.zip da caricare in $db"

$servizio = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
$eraAttivo = $servizio -and $servizio.Status -eq 'Running'
if (-not $servizio) { Write-Warning "Il servizio $ServiceName non è installato: carico solo il database." }

$codice = 0
try {
    if ($eraAttivo -and $PSCmdlet.ShouldProcess($ServiceName, 'Arresto del servizio')) {
        Write-Host "==> Arresto del servizio $ServiceName"
        Stop-Service -Name $ServiceName
        $servizio.WaitForStatus('Stopped', [TimeSpan]::FromSeconds(30))
    }
    if ($PSCmdlet.ShouldProcess($db, "Import di $($zip.Count) file")) {
        Write-Host "> $import `"$db`" <$($zip.Count) file>"
        & $import $db @($zip.FullName)
        $codice = $LASTEXITCODE
        if ($codice -eq 2) {
            Write-Warning 'Alcuni file non sono stati caricati: vedere i messaggi ERRORE qui sopra.'
        } elseif ($codice -ne 0) {
            Write-Warning "omi-import è terminato con codice $codice."
        }
    }
} finally {
    if ($eraAttivo -and $PSCmdlet.ShouldProcess($ServiceName, 'Riavvio del servizio')) {
        Write-Host "==> Riavvio del servizio $ServiceName"
        Start-Service -Name $ServiceName
        (Get-Service -Name $ServiceName).WaitForStatus('Running', [TimeSpan]::FromSeconds(30))
        Write-Host "    Servizio $ServiceName in esecuzione."
    }
}
exit $codice
