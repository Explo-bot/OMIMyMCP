# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0

<#
.SYNOPSIS
    Installa Caddy come servizio Windows, reverse proxy HTTPS davanti a OMIMyMCP.

.DESCRIPTION
    Non scarica nulla: richiede un caddy.exe già scaricato da https://caddyserver.com/download (o dalle
    release ufficiali del progetto Caddy) e ne mostra l'hash SHA-256, da confrontare con quello pubblicato.

    1. Copia caddy.exe in InstallDir e crea ConfigDir con le sottocartelle data e logs (accessibili solo
       ad Administrators e SYSTEM, perché data contiene le chiavi dei certificati).
    2. Genera ConfigDir\Caddyfile da deploy\caddy\Caddyfile.internet.example sostituendo dominio, email,
       porta del server e percorsi, ed esegue "caddy validate".
    3. Crea (o riconfigura) il servizio "caddy" con sc.exe: avvio automatico, dipendenza da OMIMyMCP,
       riavvio dopo 5, 30 e 60 secondi in caso di guasto.
    4. Crea le regole del firewall per TCP 80 (challenge HTTP-01 di Let's Encrypt) e 443 in ingresso.
    5. Avvia il servizio e verifica https://<dominio>/health.

    OMIMyMCP va installato con --bind 127.0.0.1 --trust-proxy e senza --firewall, ad esempio con
    "install.ps1 -TrustProxy". Lo script è idempotente e va eseguito come amministratore.

.PARAMETER CaddyExe
    Percorso del caddy.exe già scaricato.
.PARAMETER Dominio
    Nome DNS pubblico che punta a questa macchina (es. omi.esempio.it).
.PARAMETER Email
    Contatto per Let's Encrypt (facoltativo ma consigliato).
.PARAMETER InstallDir
    Cartella di caddy.exe. Default: C:\Program Files\Caddy.
.PARAMETER ConfigDir
    Cartella di Caddyfile, certificati (data) e log di accesso (logs). Default: C:\ProgramData\Caddy.
.PARAMETER OmiServiceName
    Nome del servizio OMIMyMCP, da cui il servizio caddy dipende. Default OMIMyMCP.
.PARAMETER Port
    Porta TCP di omi-mcp su 127.0.0.1. Default 8765.
.PARAMETER Template
    Caddyfile di partenza. Default: deploy\caddy\Caddyfile.internet.example (accanto agli script o nel repository).

.EXAMPLE
    .\install-caddy.ps1 -CaddyExe C:\Download\caddy.exe -Dominio omi.esempio.it -Email nome@esempio.it
#>
[CmdletBinding(SupportsShouldProcess)]
param(
    [Parameter(Mandatory)][string]$CaddyExe,
    [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$')][string]$Dominio,
    [ValidatePattern('^[^\s{}"]+@[^\s{}"]+$')][string]$Email,
    [string]$InstallDir = (Join-Path $env:ProgramFiles 'Caddy'),
    [string]$ConfigDir = (Join-Path $env:ProgramData 'Caddy'),
    [string]$OmiServiceName = 'OMIMyMCP',
    [ValidateRange(1, 65535)][int]$Port = 8765,
    [string]$Template
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$NomeServizio = 'caddy'
$RegoleFirewall = [ordered]@{ 'OMIMyMCP-Caddy-80' = 80; 'OMIMyMCP-Caddy-443' = 443 }

function Test-Amministratore {
    $p = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    $p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Invoke-Nativo([string]$Exe, [string[]]$Argomenti) {
    Write-Host "> $Exe $($Argomenti -join ' ')"
    & $Exe @Argomenti
    if ($LASTEXITCODE -ne 0) { throw "$Exe è terminato con codice $LASTEXITCODE." }
}

# sc.exe vuole "chiave= valore" con lo spazio dopo "=" e legge le virgolette con le regole di
# CommandLineToArgvW: la riga di comando si passa quindi intera, senza il rimaneggiamento di PowerShell.
function Invoke-Sc([string]$Riga) {
    Write-Host "> sc.exe $Riga"
    $p = Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\sc.exe') -ArgumentList $Riga `
        -NoNewWindow -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "sc.exe è terminato con codice $($p.ExitCode)." }
}

function Wait-StatoServizio([string]$Nome, [string]$Stato) {
    $s = Get-Service -Name $Nome -ErrorAction SilentlyContinue
    if ($s) { $s.WaitForStatus($Stato, [TimeSpan]::FromSeconds(30)) }
}

if (-not $WhatIfPreference -and -not (Test-Amministratore)) {
    Write-Error 'Servono i privilegi di amministratore: aprire PowerShell con "Esegui come amministratore".'
    exit 1
}

# 0. Controlli preliminari.
if (-not (Test-Path -LiteralPath $CaddyExe -PathType Leaf)) { throw "caddy.exe non trovato: $CaddyExe" }
$CaddyExe = (Resolve-Path -LiteralPath $CaddyExe).Path
if (-not $Template) {
    $Template = @(
        (Join-Path $PSScriptRoot 'Caddyfile.internet.example'),
        (Join-Path $PSScriptRoot '..\caddy\Caddyfile.internet.example')
    ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (-not $Template -or -not (Test-Path -LiteralPath $Template)) {
    throw 'Caddyfile.internet.example non trovato: indicarlo con -Template.'
}

# Con .NET, perché Get-FileHash in Windows PowerShell 5.1 rispetta -WhatIf e non restituisce nulla.
$flusso = [IO.File]::OpenRead($CaddyExe)
try {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = [BitConverter]::ToString($sha.ComputeHash($flusso)).Replace('-', '') } finally { $sha.Dispose() }
} finally { $flusso.Dispose() }
Write-Host "==> $CaddyExe"
Write-Host "    SHA-256: $hash"
Write-Host '    Confrontarlo con l''hash pubblicato per questa versione (pagina di download o release di Caddy).'

$caddyDest = Join-Path $InstallDir 'caddy.exe'
$caddyfile = Join-Path $ConfigDir 'Caddyfile'
$dataDir = Join-Path $ConfigDir 'data'
$logsDir = Join-Path $ConfigDir 'logs'

# Porte 80 e 443: se le usa un altro programma (IIS, un altro proxy) Caddy non parte.
$servizioEsistente = Get-Service -Name $NomeServizio -ErrorAction SilentlyContinue
foreach ($porta in 80, 443) {
    $occupanti = @(Get-NetTCPConnection -State Listen -LocalPort $porta -ErrorAction SilentlyContinue)
    foreach ($c in $occupanti) {
        $proc = Get-Process -Id $c.OwningProcess -ErrorAction SilentlyContinue
        if ($proc -and $proc.ProcessName -ne 'caddy') {
            Write-Warning "La porta TCP $porta è già in uso da $($proc.ProcessName) (PID $($proc.Id)): Caddy non riuscirà ad avviarsi."
        }
    }
}

# 1. Programma e cartelle. Un servizio caddy già in esecuzione blocca la sovrascrittura di caddy.exe.
if ($servizioEsistente -and $servizioEsistente.Status -ne 'Stopped') {
    if ($PSCmdlet.ShouldProcess($NomeServizio, 'Arresto del servizio esistente')) {
        Write-Host "==> Il servizio $NomeServizio esiste già: lo fermo prima di aggiornare i file."
        Stop-Service -Name $NomeServizio -Force
        Wait-StatoServizio $NomeServizio 'Stopped'
    }
}
Write-Host "==> Copia di caddy.exe in $InstallDir"
if ($PSCmdlet.ShouldProcess($InstallDir, 'Creazione della cartella e copia di caddy.exe')) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item -LiteralPath $CaddyExe -Destination $caddyDest -Force
}
Write-Host "==> Cartella di configurazione $ConfigDir"
if ($PSCmdlet.ShouldProcess($ConfigDir, 'Creazione di data e logs e ACL')) {
    New-Item -ItemType Directory -Force -Path $dataDir, $logsDir | Out-Null
    # S-1-5-32-544 = Administrators, S-1-5-18 = SYSTEM (account del servizio): data contiene le chiavi dei certificati.
    Invoke-Nativo 'icacls.exe' @($ConfigDir, '/inheritance:r', '/grant:r', '*S-1-5-32-544:(OI)(CI)F', '*S-1-5-18:(OI)(CI)F')
}

# 2. Caddyfile dall'esempio. Nei percorsi Caddy vuole la barra normale.
$configFw = $ConfigDir.TrimEnd('\').Replace('\', '/')
$testo = [IO.File]::ReadAllText($Template)
$testo = $testo.Replace('{$OMI_DOMINIO}', $Dominio)
if ($Email) {
    $testo = $testo.Replace('{$OMI_ACME_EMAIL}', $Email)
} else {
    # "email" senza argomento non è valido: senza -Email la riga si toglie.
    $testo = [regex]::Replace($testo, '(?m)^[ \t]*email \{\$OMI_ACME_EMAIL\}[ \t]*\r?\n', '')
    Write-Warning 'Nessuna -Email: Let''s Encrypt non potrà avvisare della scadenza dei certificati.'
}
$testo = $testo.Replace('C:/ProgramData/Caddy', $configFw)
$testo = $testo.Replace('127.0.0.1:8765', "127.0.0.1:$Port")
Write-Host "==> Caddyfile $caddyfile"
if ($PSCmdlet.ShouldProcess($caddyfile, 'Scrittura del Caddyfile')) {
    [IO.File]::WriteAllText($caddyfile, $testo, (New-Object Text.UTF8Encoding($false)))
}

# 3. Validazione: se fallisce non si tocca il servizio.
if ($PSCmdlet.ShouldProcess($caddyfile, 'caddy validate')) {
    Invoke-Nativo $caddyDest @('validate', '--config', $caddyfile, '--adapter', 'caddyfile')
}

# 4. Servizio, come nella documentazione ufficiale di Caddy.
$binPath = "\`"$caddyDest\`" run --config \`"$caddyfile\`" --adapter caddyfile"
$verbo = if (Get-Service -Name $NomeServizio -ErrorAction SilentlyContinue) { 'config' } else { 'create' }
Write-Host "==> Servizio $NomeServizio ($verbo)"
if ($PSCmdlet.ShouldProcess($NomeServizio, "sc.exe $verbo")) {
    Invoke-Sc "$verbo $NomeServizio start= auto binPath= `"$binPath`""
    Invoke-Sc "description $NomeServizio `"Caddy - reverse proxy HTTPS per OMIMyMCP`""
    if (Get-Service -Name $OmiServiceName -ErrorAction SilentlyContinue) {
        Invoke-Sc "config $NomeServizio depend= $OmiServiceName"
    } else {
        Write-Warning "Il servizio $OmiServiceName non esiste: dipendenza non impostata. Installarlo (install.ps1 -TrustProxy) e rilanciare questo script."
    }
    Invoke-Sc "failure $NomeServizio reset= 86400 actions= restart/5000/restart/30000/restart/60000"
}

# 5. Firewall: 80 per il challenge HTTP-01 e il reindirizzamento a HTTPS, 443 per HTTPS.
Write-Host '==> Regole del firewall (TCP 80 e 443 in ingresso)'
foreach ($nome in $RegoleFirewall.Keys) {
    if ($PSCmdlet.ShouldProcess($nome, "Regola del firewall per la porta $($RegoleFirewall[$nome])")) {
        & netsh.exe advfirewall firewall delete rule "name=$nome" | Out-Null
        Invoke-Nativo 'netsh.exe' @('advfirewall', 'firewall', 'add', 'rule', "name=$nome", 'dir=in',
            'action=allow', 'protocol=TCP', "localport=$($RegoleFirewall[$nome])")
    }
}

# 6. OMIMyMCP deve ascoltare solo in locale e leggere l'IP originale dal proxy.
$omi = Get-CimInstance Win32_Service -Filter "Name='$OmiServiceName'" -ErrorAction SilentlyContinue
if ($omi) {
    if ($omi.PathName -notmatch '--trust-proxy') {
        Write-Warning "$OmiServiceName non usa --trust-proxy: nel log del server compare solo 127.0.0.1. Reinstallarlo con install.ps1 -TrustProxy."
    }
    if ($omi.PathName -notmatch '--bind\s+"?(127\.0\.0\.1|::1)"?(\s|$)') {
        Write-Warning "$OmiServiceName non ascolta solo su 127.0.0.1: la porta $Port potrebbe essere raggiungibile senza passare da Caddy. Reinstallarlo con --bind 127.0.0.1 e senza --firewall."
    }
}
Write-Host "Promemoria: $OmiServiceName va installato con --bind 127.0.0.1 --trust-proxy e senza --firewall."

# 7. Avvio e verifica.
Write-Host "==> Avvio del servizio $NomeServizio"
if ($PSCmdlet.ShouldProcess($NomeServizio, 'Avvio del servizio e verifica di /health')) {
    Start-Service -Name $NomeServizio
    Wait-StatoServizio $NomeServizio 'Running'
    $url = "https://$Dominio/health"
    Write-Host "==> Verifica di $url (il primo certificato può richiedere un minuto)"
    $fine = (Get-Date).AddSeconds(90)
    $esito = $null
    $ultimoErrore = 'risposta diversa da 200'
    while ($true) {
        try {
            $r = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 10
            if ($r.StatusCode -eq 200) { $esito = $r; break }
        } catch {
            $ultimoErrore = $_.Exception.Message
        }
        if ((Get-Date) -ge $fine) { break }
        Start-Sleep -Seconds 5
    }
    if ($esito) {
        Write-Host "    $url risponde 200."
    } else {
        Write-Warning "$url non risponde: $ultimoErrore"
        Write-Warning "Controllare: record DNS di $Dominio, inoltro delle porte 80 e 443, servizio $OmiServiceName in esecuzione, log in $logsDir."
        exit 1
    }
}
Write-Host "Installazione di Caddy completata. URL del connettore: https://$Dominio/mcp"
