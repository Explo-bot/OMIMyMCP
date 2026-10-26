# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0

<#
.SYNOPSIS
    Disinstalla il servizio Windows OMIMyMCP.

.DESCRIPTION
    1. Ferma ed elimina il servizio e la sua regola del firewall (omi-mcp service uninstall).
    2. Con -RemoveCaddy ferma ed elimina anche il servizio caddy, le sue regole del firewall (TCP 80 e 443)
       e la cartella dei programmi di Caddy.
    3. Rimuove InstallDir e la sua voce dal PATH di sistema.
    4. Con -RemoveData elimina DataDir (database, log e token) e, se c'è -RemoveCaddy, anche CaddyConfigDir
       (Caddyfile, certificati e log di accesso), dopo una conferma esplicita che -Force salta.

    Senza -RemoveData i dati restano dove sono. Lo script è idempotente e va eseguito come amministratore.

.PARAMETER InstallDir
    Cartella dei programmi. Default: C:\Program Files\OMIMyMCP.
.PARAMETER DataDir
    Cartella dei dati. Default: C:\ProgramData\OMIMyMCP.
.PARAMETER ServiceName
    Nome del servizio. Default OMIMyMCP.
.PARAMETER RemoveData
    Elimina anche DataDir (database, log, token).
.PARAMETER RemoveCaddy
    Ferma ed elimina il servizio caddy, le regole del firewall 80/443 e CaddyInstallDir.
.PARAMETER CaddyInstallDir
    Cartella di caddy.exe. Default: C:\Program Files\Caddy.
.PARAMETER CaddyConfigDir
    Cartella di Caddyfile, certificati e log di accesso. Default: C:\ProgramData\Caddy. Si elimina solo
    con -RemoveCaddy e -RemoveData.
.PARAMETER Force
    Con -RemoveData non chiede conferma.
#>
[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$InstallDir = (Join-Path $env:ProgramFiles 'OMIMyMCP'),
    [string]$DataDir = (Join-Path $env:ProgramData 'OMIMyMCP'),
    [string]$ServiceName = 'OMIMyMCP',
    [string]$CaddyInstallDir = (Join-Path $env:ProgramFiles 'Caddy'),
    [string]$CaddyConfigDir = (Join-Path $env:ProgramData 'Caddy'),
    [switch]$RemoveData,
    [switch]$RemoveCaddy,
    [switch]$Force
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

# Ferma ed elimina un servizio senza omi-mcp.exe (sc.exe), attendendo che sparisca.
function Remove-Servizio([string]$Nome) {
    $s = Get-Service -Name $Nome -ErrorAction SilentlyContinue
    if (-not $s) { Write-Host "    Il servizio $Nome non è installato."; return }
    if ($s.Status -ne 'Stopped') {
        Write-Host "    Arresto del servizio $Nome"
        Stop-Service -Name $Nome -Force
        $s.WaitForStatus('Stopped', [TimeSpan]::FromSeconds(30))
    }
    Invoke-Nativo 'sc.exe' @('delete', $Nome)
    $fine = (Get-Date).AddSeconds(30)
    while ((Get-Service -Name $Nome -ErrorAction SilentlyContinue) -and (Get-Date) -lt $fine) {
        Start-Sleep -Milliseconds 250
    }
}

if (-not $WhatIfPreference -and -not (Test-Amministratore)) {
    Write-Error 'Servono i privilegi di amministratore: aprire PowerShell con "Esegui come amministratore".'
    exit 1
}

# 1. Servizio OMIMyMCP.
Write-Host "==> Servizio $ServiceName"
$exe = Join-Path $InstallDir 'omi-mcp.exe'
if ($PSCmdlet.ShouldProcess($ServiceName, 'Arresto ed eliminazione del servizio')) {
    if (Test-Path $exe) {
        Invoke-Nativo $exe @('service', 'uninstall', '--name', $ServiceName)
    } else {
        Write-Warning "$exe non trovato: uso sc.exe e netsh."
        Remove-Servizio $ServiceName
        # Solo la regola di omi-mcp.exe: altre regole con lo stesso nome (di altri programmi) restano.
        & netsh.exe advfirewall firewall delete rule "name=$ServiceName" "program=$exe" | Out-Null
    }
}

# 2. Caddy (facoltativo).
if ($RemoveCaddy) {
    Write-Host '==> Servizio caddy'
    if ($PSCmdlet.ShouldProcess('caddy', 'Arresto ed eliminazione del servizio')) {
        Remove-Servizio 'caddy'
    }
    # Stessi nomi creati da install-caddy.ps1.
    foreach ($regola in 'OMIMyMCP-Caddy-80', 'OMIMyMCP-Caddy-443') {
        if ($PSCmdlet.ShouldProcess($regola, 'Eliminazione della regola del firewall')) {
            & netsh.exe advfirewall firewall delete rule "name=$regola" | Out-Null
            Write-Host "    Regola $regola rimossa (se esisteva)."
        }
    }
    if (Test-Path $CaddyInstallDir) {
        if ($PSCmdlet.ShouldProcess($CaddyInstallDir, 'Eliminazione della cartella')) {
            Remove-Item -Recurse -Force -Path $CaddyInstallDir
            Write-Host "    $CaddyInstallDir eliminata."
        }
    } else {
        Write-Host "    $CaddyInstallDir non esiste."
    }
}

# 3. Programmi e PATH.
Write-Host "==> Programmi in $InstallDir"
if (Test-Path $InstallDir) {
    if ($PSCmdlet.ShouldProcess($InstallDir, 'Eliminazione della cartella')) {
        Remove-Item -Recurse -Force -Path $InstallDir
        Write-Host "    $InstallDir eliminata."
    }
} else {
    Write-Host "    $InstallDir non esiste."
}
$path = [Environment]::GetEnvironmentVariable('Path', 'Machine')
$voci = @($path -split ';' | Where-Object { $_ -ne '' })
$restanti = @($voci | Where-Object { $_.TrimEnd('\') -ne $InstallDir.TrimEnd('\') })
if ($restanti.Count -ne $voci.Count -and $PSCmdlet.ShouldProcess('PATH di sistema', "Rimozione di $InstallDir")) {
    [Environment]::SetEnvironmentVariable('Path', ($restanti -join ';'), 'Machine')
    Write-Host "    $InstallDir rimossa dal PATH di sistema."
}

# 4. Dati (facoltativo, con conferma).
if ($RemoveData) {
    Write-Host "==> Dati in $DataDir"
    if (-not (Test-Path $DataDir)) {
        Write-Host "    $DataDir non esiste."
    } elseif ($Force -or $WhatIfPreference -or $PSCmdlet.ShouldContinue(
            "Eliminare definitivamente $DataDir (database, log e token)?", 'Conferma')) {
        if ($PSCmdlet.ShouldProcess($DataDir, 'Eliminazione della cartella dei dati')) {
            Remove-Item -Recurse -Force -Path $DataDir
            Write-Host "    $DataDir eliminata."
        }
    } else {
        Write-Host "    Dati lasciati in $DataDir."
    }
    # Certificati, Caddyfile e log di accesso di Caddy: solo se Caddy è stato rimosso, e solo con la stessa conferma.
    if ($RemoveCaddy -and (Test-Path $CaddyConfigDir)) {
        if ($Force -or $WhatIfPreference -or $PSCmdlet.ShouldContinue(
                "Eliminare definitivamente $CaddyConfigDir (Caddyfile, certificati e log di accesso)?", 'Conferma')) {
            if ($PSCmdlet.ShouldProcess($CaddyConfigDir, 'Eliminazione della cartella di configurazione di Caddy')) {
                Remove-Item -Recurse -Force -Path $CaddyConfigDir
                Write-Host "    $CaddyConfigDir eliminata."
            }
        } else {
            Write-Host "    Configurazione di Caddy lasciata in $CaddyConfigDir."
        }
    }
} else {
    Write-Host "I dati restano in $DataDir (database, log e token): per eliminarli usare -RemoveData."
    if ($RemoveCaddy) {
        Write-Host "La configurazione di Caddy resta in $CaddyConfigDir (certificati e log): per eliminarla usare -RemoveData."
    }
}
Write-Host 'Disinstallazione completata.'
