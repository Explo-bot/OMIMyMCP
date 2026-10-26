# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0

<#
.SYNOPSIS
    Verifica, in sola lettura, che dopo uninstall.ps1 non restino servizi, regole del firewall,
    cartelle o voci del PATH di OMIMyMCP.

.DESCRIPTION
    Controlla il servizio di produzione e quello di prova (OMIMyMCP-Test), le regole del firewall con
    lo stesso nome e quelle di Caddy, la cartella dei programmi, quella dei dati (salvo -DatiMantenuti),
    la cartella della prova del servizio, il PATH di sistema e la porta in ascolto. Non modifica nulla e
    non richiede privilegi di amministratore. Esce con 0 se non ci sono residui, altrimenti con 1.

.PARAMETER ServiceName
    Nome del servizio. Default OMIMyMCP.
.PARAMETER InstallDir
    Cartella dei programmi. Default: C:\Program Files\OMIMyMCP.
.PARAMETER DataDir
    Cartella dei dati. Default: C:\ProgramData\OMIMyMCP.
.PARAMETER Port
    Porta del server. Default 8765.
.PARAMETER DatiMantenuti
    La cartella dei dati è stata mantenuta di proposito (uninstall.ps1 senza -RemoveData): la sua presenza
    non conta come residuo.
#>
[CmdletBinding()]
param(
    [string]$ServiceName = 'OMIMyMCP',
    [string]$InstallDir = (Join-Path $env:ProgramFiles 'OMIMyMCP'),
    [string]$DataDir = (Join-Path $env:ProgramData 'OMIMyMCP'),
    [int]$Port = 8765,
    [switch]$DatiMantenuti
)

Set-StrictMode -Version Latest
$script:residui = 0

function Test-Assente([string]$Descrizione, [object]$Trovato) {
    if ($null -eq $Trovato -or @($Trovato).Count -eq 0) {
        Write-Host "[OK]      ${Descrizione}: assente" -ForegroundColor Green
    } else {
        $script:residui++
        Write-Host "[RESIDUO] ${Descrizione}: $(@($Trovato) -join ', ')" -ForegroundColor Red
    }
}

foreach ($nome in @($ServiceName, 'OMIMyMCP-Test')) {
    Test-Assente "Servizio $nome" (Get-Service -Name $nome -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Name) ($($_.Status))" })
}
# Le regole con lo stesso nome ma di un altro programma (per esempio create da Windows quando si consente
# l'accesso a un eseguibile con la stessa descrizione) non sono residui: le si elenca soltanto.
foreach ($regola in @($ServiceName, 'OMIMyMCP-Test', 'OMIMyMCP-Caddy-80', 'OMIMyMCP-Caddy-443')) {
    $nostre = @()
    foreach ($r in @(Get-NetFirewallRule -DisplayName $regola -ErrorAction SilentlyContinue)) {
        $programma = ($r | Get-NetFirewallApplicationFilter).Program
        if ($regola -like 'OMIMyMCP-Caddy-*' -or $programma -like '*\omi-mcp.exe') {
            $nostre += "$regola ($programma)"
        } else {
            Write-Host "[INFO]    Regola del firewall $regola di un altro programma, non toccata: $programma"
        }
    }
    Test-Assente "Regola del firewall $regola di omi-mcp" $nostre
}
$cartelle = @($InstallDir, (Join-Path $env:ProgramData 'OMIMyMCP-Test'))
if (-not $DatiMantenuti) { $cartelle += $DataDir }
foreach ($cartella in $cartelle) {
    Test-Assente "Cartella $cartella" (Get-Item -LiteralPath $cartella -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })
}
if ($DatiMantenuti) {
    $presente = Test-Path -LiteralPath $DataDir
    Write-Host "[INFO]    Cartella dei dati $DataDir mantenuta di proposito: $(if ($presente) { 'presente' } else { 'assente' })"
}
$voci = [Environment]::GetEnvironmentVariable('Path', 'Machine') -split ';' | Where-Object { $_ -and ($_.TrimEnd('\') -ieq $InstallDir.TrimEnd('\')) }
Test-Assente "Voce del PATH di sistema $InstallDir" $voci
Test-Assente "Processo in ascolto sulla porta $Port" (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue | ForEach-Object { "PID $($_.OwningProcess)" })

if ($script:residui -eq 0) {
    Write-Host 'Nessun residuo.' -ForegroundColor Green
    exit 0
}
Write-Host "Residui trovati: $script:residui." -ForegroundColor Red
exit 1
