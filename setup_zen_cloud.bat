@echo off
chcp 65001 >nul
title Construtor ZenMonitor Cloud v2.0
echo ===================================================
echo   CRIANDO ESTRUTURA LIMPA - ZEN MONITOR CLOUD
echo ===================================================

:: 1. Criando pastas da arquitetura local
mkdir local_agent 2>nul
mkdir local_agent\core 2>nul
mkdir local_agent\services 2>nul

:: 2. Criando .gitignore e .dockerignore
(
echo .venv/
echo venv/
echo __pycache__/
echo *.pyc
echo .env
) > .gitignore

(
echo .venv
echo venv
echo __pycache__
echo local_agent
echo .git
echo *.bat
) > .dockerignore

:: 3. Criando requirements.txt (Unificado e leve para a VPS)
(
echo fastapi==0.115.0
echo uvicorn[standard]==0.30.6
echo pydantic==2.9.2
echo requests==2.32.3
) > requirements.txt

:: 4. Criando Dockerfile na raiz (Padrao Oficial Coolify)
(
echo FROM python:3.11-slim
echo WORKDIR /app
echo COPY requirements.txt .
echo RUN pip install --no-cache-dir -r requirements.txt
echo COPY vps_app.py .
echo EXPOSE 8000
echo CMD ["python", "-m", "uvicorn", "vps_app:app", "--host", "0.0.0.0", "--port", "8000"]
) > Dockerfile

:: 5. Criando Servidor Central + Dashboard Web Integrado (vps_app.py)
(
echo from fastapi import FastAPI
echo from fastapi.responses import HTMLResponse
echo from pydantic import BaseModel
echo from typing import Dict, Any, List
echo import time
echo.
echo app = FastAPI^(title="ZenMonitor Cloud", version="2.0"^)
echo.
echo # Armazenamento em memoria: { "machine_id": { "last_seen": float, "clientes": [...] } }
echo DADOS_MAQUINAS: Dict[str, Dict[str, Any]] = {}
echo TIMEOUT_MAQUINA = 30  # Segundos ate considerar o PC offline
echo.
echo class PayloadRelatorio^(BaseModel^):
echo     machine_id: str
echo     clientes: List[Dict[str, Any]]
echo.
echo @app.get^("/health"^)
echo def health_check^(^):
echo     return {"status": "ok", "timestamp": time.time^(^)}
echo.
echo @app.post^("/api/v1/report"^)
echo def receber_relatorio^(payload: PayloadRelatorio^):
echo     DADOS_MAQUINAS[payload.machine_id] = {
echo         "last_seen": time.time^(^),
echo         "clientes": payload.clientes
echo     }
echo     return {"status": "ok", "recebidos": len^(payload.clientes^)}
echo.
echo @app.get^("/api/v1/aggregate"^)
echo def obter_dados^(^):
echo     agora = time.time^(^)
echo     ativas = {}
echo     total_contas = 0
echo     total_zen = 0
echo.
echo     for m_id, info in list^(DADOS_MAQUINAS.items^(^)^):
echo         atraso = int^(agora - info["last_seen"]^)
echo         if atraso ^<= TIMEOUT_MAQUINA:
echo             ativas[m_id] = {
echo                 "ultimo_envio_seg": atraso,
echo                 "clientes": info["clientes"]
echo             }
echo             total_contas += len^(info["clientes"]^)
echo             for c in info["clientes"]:
echo                 val = c.get^("zen", 0^)
echo                 if isinstance^(val, int^):
echo                     total_zen += val
echo.
echo     return {
echo         "total_maquinas": len^(ativas^),
echo         "total_contas": total_contas,
echo         "total_zen": total_zen,
echo         "maquinas": ativas
echo     }
echo.
echo @app.get^("/", response_class=HTMLResponse^)
echo def dashboard^(^):
echo     return """
echo     ^<!DOCTYPE html^>
echo     ^<html lang="pt-BR"^>
echo     ^<head^>
echo         ^<meta charset="UTF-8"^>
echo         ^<meta name="viewport" content="width=device-width, initial-scale=1.0"^>
echo         ^<title^>ZenMonitor Cloud^</title^>
echo         ^<script src="https://cdn.tailwindcss.com"^>^</script^>
echo     ^</head^>
echo     ^<body class="bg-slate-900 text-slate-100 min-h-screen p-6"^>
echo         ^<div class="max-w-6xl mx-auto"^>
echo             ^<header class="flex flex-col md:flex-row justify-between items-center mb-8 border-b border-slate-700 pb-4"^>
echo                 ^<div^>
echo                     ^<h1 class="text-2xl font-bold text-emerald-400"^>⚡ ZenMonitor Cloud^</h1^>
echo                     ^<p class="text-slate-400 text-sm"^>Monitoramento distribuído em tempo real^</p^>
echo                 ^</div^>
echo                 ^<div class="flex gap-4 mt-4 md:mt-0"^>
echo                     ^<div class="bg-slate-800 px-4 py-2 rounded-lg border border-slate-700 text-center"^>
echo                         ^<span class="text-xs text-slate-400 block"^>PCs Online^</span^>
echo                         ^<span id="tot-pc" class="text-xl font-bold text-white"^>0^</span^>
echo                     ^</div^>
echo                     ^<div class="bg-slate-800 px-4 py-2 rounded-lg border border-slate-700 text-center"^>
echo                         ^<span class="text-xs text-slate-400 block"^>Contas Ativas^</span^>
echo                         ^<span id="tot-acc" class="text-xl font-bold text-emerald-400"^>0^</span^>
echo                     ^</div^>
echo                     ^<div class="bg-slate-800 px-4 py-2 rounded-lg border border-slate-700 text-center"^>
echo                         ^<span class="text-xs text-slate-400 block"^>Zen Total^</span^>
echo                         ^<span id="tot-zen" class="text-xl font-bold text-amber-400"^>0^</span^>
echo                     ^</div^>
echo                 ^</div^>
echo             ^</header^>
echo             ^<div id="grid-maquinas" class="grid grid-cols-1 md:grid-cols-2 gap-6"^>^</div^>
echo         ^</div^>
echo         ^<script^>
echo             const formatZen = ^(n^) =^> n ? n.toLocaleString^('pt-BR'^) : '0';
echo             async function atualizar^(^) {
echo                 try {
echo                     const res = await fetch^('/api/v1/aggregate'^);
echo                     const data = await res.json^(^);
echo                     document.getElementById^('tot-pc'^).innerText = data.total_maquinas;
echo                     document.getElementById^('tot-acc'^).innerText = data.total_contas;
echo                     document.getElementById^('tot-zen'^).innerText = formatZen^(data.total_zen^);
echo                     const container = document.getElementById^('grid-maquinas'^);
echo                     container.innerHTML = '';
echo                     if ^(data.total_maquinas === 0^) {
echo                         container.innerHTML = '^<div class="col-span-2 text-center py-12 bg-slate-800/50 rounded-xl border border-slate-800 text-slate-400"^>Nenhuma máquina reportando no momento. Inicie o agent_sender.py nos PCs locais.^</div^>';
echo                         return;
echo                     }
echo                     for ^(const [pc, info] of Object.entries^(data.maquinas^)^) {
echo                         let linhas = '';
echo                         for ^(const c of info.clientes^) {
echo                             linhas += `
echo                                 ^<tr class="border-b border-slate-700/50 last:border-0"^>
echo                                     ^<td class="py-2 text-slate-400 text-sm"^>#${c.pid}^</td^>
echo                                     ^<td class="py-2 font-medium text-white"^>${c.name}^</td^>
echo                                     ^<td class="py-2 text-amber-400 font-mono font-semibold text-right"^>${formatZen^(c.zen^)}^</td^>
echo                                     ^<td class="py-2 text-right"^>^<span class="px-2 py-0.5 text-xs rounded bg-emerald-500/20 text-emerald-300"^>${c.status ^|^| 'OK'}^</span^>^</td^>
echo                                 ^</tr^>`;
echo                         }
echo                         container.innerHTML += `
echo                             ^<div class="bg-slate-800 rounded-xl p-5 border border-slate-700 shadow-lg"^>
echo                                 ^<div class="flex justify-between items-center mb-4 pb-2 border-b border-slate-700"^>
echo                                     ^<h2 class="text-lg font-bold text-emerald-400 flex items-center gap-2"^>🖥️ ${pc}^</h2^>
echo                                     ^<span class="text-xs text-slate-400"^>há ${info.ultimo_envio_seg}s^</span^>
echo                                 ^</div^>
echo                                 ^<table class="w-full text-left"^>
echo                                     ^<thead^>
echo                                         ^<tr class="text-xs text-slate-400 border-b border-slate-700"^>
echo                                             ^<th class="pb-2"^>PID^</th^>
echo                                             ^<th class="pb-2"^>Personagem^</th^>
echo                                             ^<th class="pb-2 text-right"^>Zen^</th^>
echo                                             ^<th class="pb-2 text-right"^>Status^</th^>
echo                                         ^</tr^>
echo                                     ^</thead^>
echo                                     ^<tbody^>${linhas}^</tbody^>
echo                                 ^</table^>
echo                             ^</div^>`;
echo                     }
echo                 } catch ^(e^) { console.error^(e^); }
echo             }
echo             atualizar^(^);
echo             setInterval^(atualizar, 3000^);
echo         ^</script^>
echo     ^</body^>
echo     ^</html^>
echo     """
) > vps_app.py

:: 6. Criando arquivos base dos modulos locais
type nul > local_agent\__init__.py
type nul > local_agent\core\__init__.py
type nul > local_agent\services\__init__.py

(
echo class MonitorService:
echo     """
echo     Substitua este arquivo pelo seu monitor_service.py real 
echo     que chama o core/memory.py, core/windows.py e core/ocr.py.
echo     """
echo     def coletar_contas^(self^):
echo         # Retorno de teste inicial para validar a conexao com a VPS de imediato
echo         return [
echo             {"pid": 1024, "name": "CharTeste_1", "resolucao": "800x600", "zen": 154000000, "status": "Online"},
echo             {"pid": 2048, "name": "CharTeste_2", "resolucao": "800x600", "zen": 89500000, "status": "Online"}
echo         ]
) > local_agent\services\monitor_service.py

:: 7. Criando o Agente de Envio Local (agent_sender.py)
(
echo import time
echo import socket
echo import requests
echo from local_agent.services.monitor_service import MonitorService
echo.
echo VPS_URL = "https://zen.brautosystem.com.br/api/v1/report"
echo MACHINE_ID = socket.gethostname^(^)
echo INTERVALO_SEGUNDOS = 5
echo.
echo def iniciar_agente^(^):
echo     monitor = MonitorService^(^)
echo     print^(f"=================================================="^)
echo     print^(f" Agente Iniciado na Maquina: {MACHINE_ID}"^)
echo     print^(f" Destino VPS: {VPS_URL}"^)
echo     print^(f"=================================================="^)
echo.
echo     while True:
echo         try:
echo             contas = monitor.coletar_contas^(^)
echo             payload = {
echo                 "machine_id": MACHINE_ID,
echo                 "clientes": contas
echo             }
echo             res = requests.post^(VPS_URL, json=payload, timeout=5^)
echo             if res.status_code == 200:
echo                 print^(f"[{time.strftime('%%H:%%M:%%S')}] OK! {len(contas)} conta(s) enviada(s) para a VPS."^)
echo             else:
echo                 print^(f"[{time.strftime('%%H:%%M:%%S')}] Erro HTTP {res.status_code}: {res.text}"^)
echo         except Exception as e:
echo             print^(f"[{time.strftime('%%H:%%M:%%S')}] Falha de conexao com a VPS: {e}"^)
echo.
echo         time.sleep^(INTERVALO_SEGUNDOS^)
echo.
echo if __name__ == "__main__":
echo     iniciar_agente^(^)
) > agent_sender.py

:: 8. Criando atalho para rodar o agente nos PCs locais
(
echo @echo off
echo title Agente Local - ZenMonitor
echo python agent_sender.py
echo pause
) > iniciar_agente_local.bat

:: 9. Inicializando Git limpo
git init
git branch -M main
git add .
git commit -m "feat: estrutura limpa ZenMonitor v2.0 com Dashboard e API unificados para Coolify"

echo.
echo ===================================================
echo   PROJETO CRIADO E COMMITADO COM SUCESSO!
echo ===================================================
pause