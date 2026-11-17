<#
ZenMonitor — comando único.

  .\zen.cmd build            testa o núcleo, compila e monta a pasta dist\
  .\zen.cmd start            abre TODOS os módulos: central + agente + painel no navegador
  .\zen.cmd start -Teste     o mesmo, com o fake-client e o agente só no char "Teste"
  .\zen.cmd start -SemNavegador   não abre o painel no navegador (para automação)
  .\zen.cmd stop             fecha agente, central e fake-client
  .\zen.cmd restart [-Teste] stop + start
  .\zen.cmd status           estado de cada módulo e das automações
  .\zen.cmd token on|off     liga/desliga a automação de token (central e agente)
  .\zen.cmd install          sobe central e agente sozinhos no logon (tarefas agendadas)
  .\zen.cmd uninstall        remove as tarefas agendadas
  .\zen.cmd chave            cria a chave de assinatura das atualizações (uma vez)
  .\zen.cmd publicar         compila o agente e publica para TODOS os PCs (atualização automática)
  .\zen.cmd publicar-teste   compila e publica SÓ para as test_machines do central.toml (canal de teste)

Não liga a leitura do chat/PK (varre a memória do jogo): isso é decisão sua, no agent.toml.
#>
param(
    [Parameter(Position = 0)][string]$Acao = "status",
    [Parameter(Position = 1)][string]$Valor = "",
    [switch]$Teste,
    [switch]$SemNavegador,
    [string]$Dir = ""
)
$ErrorActionPreference = "Stop"

function Find-Bin {
    if ($Dir) { return (Resolve-Path $Dir).Path }
    foreach ($d in @("$PSScriptRoot\target\release", $PSScriptRoot, "$PSScriptRoot\dist")) {
        if (Test-Path (Join-Path $d "zen-central.exe")) { return $d }
    }
    return "$PSScriptRoot\target\release"
}
$Bin = Find-Bin
$Url = "http://127.0.0.1:8787"

function Info($m) { Write-Host "  $m" }
function Ok($m)   { Write-Host "  [OK]    $m" -ForegroundColor Green }
function Warn($m) { Write-Host "  [AVISO] $m" -ForegroundColor Yellow }
function Bad($m)  { Write-Host "  [ERRO]  $m" -ForegroundColor Red }

function Test-Admin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    return (New-Object Security.Principal.WindowsPrincipal $id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-Running($name) { return @(Get-Process -Name $name -ErrorAction SilentlyContinue) }

function Get-State {
    try { return Invoke-RestMethod -Uri "$Url/api/state" -TimeoutSec 3 } catch { return $null }
}

function Read-TomlValue($file, $key) {
    if (-not (Test-Path $file)) { return $null }
    foreach ($line in Get-Content $file -Encoding UTF8) {
        if ($line -match "^\s*$key\s*=\s*(.+?)\s*$") { return $Matches[1].Trim('"') }
    }
    return $null
}

function Set-TomlBool($file, $key, [bool]$value) {
    if (-not (Test-Path $file)) { return $false }
    $v = if ($value) { "true" } else { "false" }
    $lines = @(Get-Content $file -Encoding UTF8)
    $found = $false
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match "^\s*$key\s*=") { $lines[$i] = "$key = $v"; $found = $true }
    }
    if (-not $found) { $lines = @("$key = $v") + $lines }
    $tmp = "$file.tmp"
    [IO.File]::WriteAllLines($tmp, $lines, (New-Object Text.UTF8Encoding $false))
    Move-Item -Force $tmp $file
    return $true
}

function Do-Build {
    Push-Location $PSScriptRoot
    try {
        Info "Testando o núcleo..."
        cargo test -p zen-core
        if ($LASTEXITCODE -ne 0) { Bad "testes do núcleo falharam"; exit 1 }
        Info "Testando o central (inclui teste de ponta a ponta)..."
        cargo test -p zen-central
        if ($LASTEXITCODE -ne 0) { Bad "testes do central falharam"; exit 1 }
        Info "Testando o agente..."
        cargo test -p zen-agent
        if ($LASTEXITCODE -ne 0) { Bad "testes do agente falharam"; exit 1 }
        Ok "todos os testes passaram"

        # O Windows não deixa substituir um .exe aberto: fecha os módulos antes de compilar.
        Info "Fechando os módulos abertos para trocar os executáveis..."
        Do-Stop
        if (Test-Locked) { Bad "ainda há módulo aberto; feche-o (ou rode como administrador) e tente de novo"; exit 1 }

        Info "Compilando (release)..."
        cargo build --release
        if ($LASTEXITCODE -ne 0) { Bad "compilação falhou"; exit 1 }
        $dist = Join-Path $PSScriptRoot "dist"
        New-Item -ItemType Directory -Force $dist | Out-Null
        foreach ($f in @("zen-central.exe", "zen-agent.exe", "zen-diag.exe", "fake-client.exe")) {
            Copy-Item -Force (Join-Path "$PSScriptRoot\target\release" $f) $dist
        }
        Copy-Item -Force "$PSScriptRoot\zen.ps1", "$PSScriptRoot\zen.cmd", "$PSScriptRoot\variaveis-exemplo.txt" $dist
        Copy-Item -Force "$PSScriptRoot\scripts\instalar-agente.ps1", "$PSScriptRoot\scripts\instalar-agente.cmd" $dist
        Ok "pronto em $dist"
        Info "Para abrir tudo: .\zen.cmd start   (ou .\zen.cmd start -Teste)"
    } finally { Pop-Location }
}

function Do-Stop {
    $names = @("zen-agent", "zen-central", "fake-client")
    $denied = @()
    foreach ($n in $names) {
        $p = Get-Running $n
        if ($p.Count -eq 0) { continue }
        try { $p | Stop-Process -Force -ErrorAction Stop; Ok "$n parado ($($p.Count) processo(s))" }
        catch { $denied += $n }
    }
    if ($denied.Count -gt 0) {
        # O agente costuma rodar como administrador; daqui sem elevação não dá para fechá-lo.
        if (Test-Admin) {
            Bad "não consegui parar: $($denied -join ', ')"
        } else {
            Info "Fechando como administrador (o Windows vai pedir confirmação): $($denied -join ', ')"
            $tk = @()
            foreach ($n in $denied) { $tk += "/IM"; $tk += "$n.exe" }
            $tk += "/F"
            try { Start-Process -FilePath "taskkill.exe" -ArgumentList $tk -Verb RunAs -Wait -WindowStyle Hidden }
            catch { Bad "fechamento como administrador recusado: $($_.Exception.Message)" }
        }
    }
    # O Windows só libera o .exe depois que o processo termina de sair.
    for ($i = 0; $i -lt 40; $i++) {
        $left = 0
        foreach ($n in $names) { $left += (Get-Running $n).Count }
        if ($left -eq 0) { break }
        Start-Sleep -Milliseconds 250
    }
    foreach ($n in $denied) {
        if ((Get-Running $n).Count -eq 0) { Ok "$n parado (como administrador)" } else { Bad "$n continua rodando" }
    }
}

function Test-Locked {
    # true se algum módulo ainda está aberto (o build não conseguiria trocar o .exe).
    foreach ($n in @("zen-agent", "zen-central", "fake-client")) {
        if ((Get-Running $n).Count -gt 0) { return $true }
    }
    return $false
}

function Do-Start {
    foreach ($f in @("zen-central.exe", "zen-agent.exe")) {
        if (-not (Test-Path (Join-Path $Bin $f))) { Bad "$f não encontrado em $Bin. Rode: .\zen.cmd build"; exit 1 }
    }
    # Sempre começa limpo: cópias antigas escondidas foram o problema nº 1 nos testes.
    Do-Stop

    Info "Subindo o central..."
    Start-Process -FilePath (Join-Path $Bin "zen-central.exe") -WorkingDirectory $Bin -WindowStyle Minimized
    $up = $false
    for ($i = 0; $i -lt 30; $i++) {
        Start-Sleep -Milliseconds 500
        if (Get-State) { $up = $true; break }
    }
    if ($up) { Ok "central no ar em $Url" } else { Bad "central não respondeu em 15 s (veja $Bin\logs\central.*.log)"; exit 1 }

    $agentArgs = @()
    if ($Teste) {
        $fc = Join-Path $Bin "fake-client.exe"
        if (Test-Path $fc) {
            Start-Process -FilePath $fc -ArgumentList "Teste", "423485898" -WorkingDirectory $Bin
            Ok "fake-client aberto (char Teste)"
        } else { Warn "fake-client.exe não encontrado" }
        $agentArgs = @("--teste")
    }

    Info "Subindo o agente (pede administrador: o jogo costuma rodar elevado)..."
    $ag = @{ FilePath = (Join-Path $Bin "zen-agent.exe"); WorkingDirectory = $Bin }
    if ($agentArgs.Count -gt 0) { $ag.ArgumentList = $agentArgs }
    if (-not (Test-Admin)) { $ag.Verb = "RunAs" }
    try { Start-Process @ag; Ok "agente iniciado$(if ($Teste) { ' em MODO TESTE (só o char Teste)' })" }
    catch { Bad "agente não iniciou (UAC recusado?): $($_.Exception.Message)" }

    # Uma aba só (dono, 12/11): a aba do painel que já estava aberta se reconecta e se recarrega
    # sozinha em poucos segundos. Só abre outra se nenhuma aparecer.
    if (-not $SemNavegador) {
        $abertas = 0
        for ($i = 0; $i -lt 4 -and $abertas -eq 0; $i++) {
            Start-Sleep -Seconds 2
            try { $abertas = [int](Invoke-RestMethod -Uri "$Url/api/panels" -TimeoutSec 3).open } catch { $abertas = 0 }
        }
        if ($abertas -gt 0) { Ok "painel já aberto no navegador ($abertas aba(s)): atualizado, sem abrir outra" }
        else { Start-Process $Url; Ok "painel aberto no navegador" }
    }
    Do-Status
}

function Do-Status {
    Write-Host ""
    Write-Host "ZenMonitor — módulos ($Bin)" -ForegroundColor Cyan
    $st = Get-State
    if ($st) { Ok "central: no ar ($Url)" }
    elseif ((Get-Running "zen-central").Count -gt 0) { Warn "central: processo aberto mas sem resposta" }
    else { Bad "central: parado" }

    $ag = Get-Running "zen-agent"
    if ($ag.Count -gt 0) { Ok "agente: rodando ($($ag.Count) processo(s): vigia + trabalhador)" } else { Bad "agente: parado" }
    if ((Get-Running "fake-client").Count -gt 0) { Info "fake-client: aberto" }

    if ($st) {
        $auto = if ($st.auto_pairing) { "LIGADA" } else { "DESLIGADA" }
        Info "automação de token no central: $auto"
        foreach ($m in @($st.machines)) {
            if (-not $m) { continue }
            $c = if ($m.connected) { "conectado" } else { "desconectado" }
            Info "máquina $($m.name): $c, versão $($m.version)"
            foreach ($ch in @($m.chars)) { if ($ch) { Info "    $($ch.name): $($ch.lock)" } }
            if ($m.last_error) { Warn "    último erro: $($m.last_error)" }
        }
        if (@($st.chars).Count -eq 0) { Warn "nenhum char no painel ainda" }
    }

    $agentToml = Join-Path $Bin "agent.toml"
    if (Test-Path $agentToml) {
        $at = Read-TomlValue $agentToml "auto_token"; if (-not $at) { $at = "true (padrão)" }
        Info "agent.toml: auto_token=$at, chat_enabled=$(Read-TomlValue $agentToml 'chat_enabled'), ram_paused=$(Read-TomlValue $agentToml 'ram_paused')"
        if ((Read-TomlValue $agentToml "chat_enabled") -eq "true") { Warn "leitura do chat/PK LIGADA (varre a memória do jogo a cada 60 s)" }
        else { Info "leitura do chat/PK: desligada (decisão 0005; liga em agent.toml: chat_enabled = true)" }
    }
    $centralToml = Join-Path $Bin "central.toml"
    if (Test-Path $centralToml) {
        $ntfy = Read-TomlValue $centralToml "ntfy_url"; $tg = Read-TomlValue $centralToml "telegram_bot_token"
        $envC = Join-Path $Bin ".env"
        if (-not $ntfy -and (Test-Path $envC) -and (Select-String -Path $envC -Pattern '^\s*ZEN_NTFY_URL\s*=\s*\S' -Quiet)) { $ntfy = "env" }
        if ($ntfy -or $tg) { Ok "avisos no celular: configurados" } else { Warn "avisos no celular: desligados (ZEN_NTFY_URL no .env ou ntfy_url/telegram no central.toml)" }
    }
    Write-Host ""
}

function Do-Token($v) {
    $on = switch ($v.ToLower()) { "on" { $true } "ligar" { $true } "off" { $false } "desligar" { $false } default { $null } }
    if ($null -eq $on) { Bad "use: .\zen.cmd token on   ou   .\zen.cmd token off"; exit 1 }
    $centralToml = Join-Path $Bin "central.toml"
    $agentToml = Join-Path $Bin "agent.toml"
    if (Get-State) {
        $admin = Read-TomlValue $centralToml "admin_token"
        try {
            Invoke-RestMethod -Method Post -Uri "$Url/api/auto_pairing" -TimeoutSec 5 -ContentType "application/json" `
                -Headers @{ Authorization = "Bearer $admin" } -Body (@{ enabled = $on } | ConvertTo-Json) | Out-Null
            Ok "central: automação de token $(if ($on) {'LIGADA'} else {'DESLIGADA'}) (já valendo)"
        } catch { Bad "central não aceitou: $($_.Exception.Message)" }
    } elseif (Set-TomlBool $centralToml "auto_pairing" $on) { Ok "central.toml: auto_pairing = $on (vale ao subir)" }
    if (Set-TomlBool $agentToml "auto_token" $on) { Ok "agent.toml: auto_token = $on (vale ao reiniciar o agente)" }
    if (-not $on) { Info "modo manual: copie o token de cada máquina do central.toml para o agent.toml dela" }
}

function Do-Install {
    if (-not (Test-Admin)) { Bad "rode como administrador"; exit 1 }
    $user = "$env:USERDOMAIN\$env:USERNAME"
    $set = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1)
    $pr = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Highest
    $tr = New-ScheduledTaskTrigger -AtLogOn -User $user
    $c = New-ScheduledTaskAction -Execute (Join-Path $Bin "zen-central.exe") -WorkingDirectory $Bin
    Register-ScheduledTask -TaskName "ZenMonitor Central" -Action $c -Trigger $tr -Principal $pr -Settings $set -Force | Out-Null
    Ok "tarefa 'ZenMonitor Central' criada (no logon)"
    $a = New-ScheduledTaskAction -Execute (Join-Path $Bin "zen-agent.exe") -WorkingDirectory $Bin
    Register-ScheduledTask -TaskName "ZenMonitor Agent" -Action $a -Trigger $tr -Principal $pr -Settings $set -Force | Out-Null
    Ok "tarefa 'ZenMonitor Agent' criada (no logon, com privilégios mais altos)"
    Info "Em PCs que só têm o jogo, remova a tarefa do central: Unregister-ScheduledTask 'ZenMonitor Central'"
}

function Do-Key {
    & (Join-Path $Bin "zen-central.exe") chave-nova
}

# Compila o agente numa pasta à parte (o zen-agent.exe deste PC está em uso) e publica assinado.
function Do-Publish([switch]$Teste) {
    $t = Join-Path $PSScriptRoot "target\publicar"
    Push-Location $PSScriptRoot
    try {
        Info "Compilando o agente para publicar..."
        cargo build --release -p zen-agent --target-dir $t
        if ($LASTEXITCODE -ne 0) { Bad "compilação falhou"; exit 1 }
    } finally { Pop-Location }
    if ($Teste) { & (Join-Path $Bin "zen-central.exe") publicar-agente (Join-Path $t "release\zen-agent.exe") --teste }
    else { & (Join-Path $Bin "zen-central.exe") publicar-agente (Join-Path $t "release\zen-agent.exe") }
    if ($LASTEXITCODE -ne 0) { Bad "publicação falhou"; exit 1 }
    Ok "publicado"
}

function Do-Uninstall {
    foreach ($t in @("ZenMonitor Central", "ZenMonitor Agent")) {
        if (Get-ScheduledTask -TaskName $t -ErrorAction SilentlyContinue) {
            Unregister-ScheduledTask -TaskName $t -Confirm:$false; Ok "tarefa '$t' removida"
        }
    }
}

switch ($Acao.ToLower()) {
    "build"     { Do-Build }
    "start"     { Do-Start }
    "stop"      { Do-Stop }
    "restart"   { Do-Stop; Do-Start }
    "status"    { Do-Status }
    "token"     { Do-Token $Valor }
    "install"   { Do-Install }
    "uninstall" { Do-Uninstall }
    "chave"     { Do-Key }
    "publicar"  { Do-Publish }
    "publicar-teste" { Do-Publish -Teste }
    default     { Get-Help $PSCommandPath; exit 1 }
}
