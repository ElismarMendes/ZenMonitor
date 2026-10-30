from fastapi import FastAPI
from fastapi.responses import HTMLResponse
from pydantic import BaseModel
from typing import Dict, Any, List
import time

app = FastAPI(title="ZenMonitor Cloud", version="2.0")

# Armazenamento em memoria: { "machine_id": { "last_seen": float, "clientes": [...] } }
DADOS_MAQUINAS: Dict[str, Dict[str, Any]] = {}
TIMEOUT_MAQUINA = 30  # Segundos ate considerar o PC offline

class PayloadRelatorio(BaseModel):
    machine_id: str
    clientes: List[Dict[str, Any]]

@app.get("/health")
def health_check():
    return {"status": "ok", "timestamp": time.time()}

@app.post("/api/v1/report")
def receber_relatorio(payload: PayloadRelatorio):
    DADOS_MAQUINAS[payload.machine_id] = {
        "last_seen": time.time(),
        "clientes": payload.clientes
    }
    return {"status": "ok", "recebidos": len(payload.clientes)}

@app.get("/api/v1/aggregate")
def obter_dados():
    agora = time.time()
    ativas = {}
    total_contas = 0
    total_zen = 0

    for m_id, info in list(DADOS_MAQUINAS.items()):
        atraso = int(agora - info["last_seen"])
        if atraso <= TIMEOUT_MAQUINA:
            ativas[m_id] = {
                "ultimo_envio_seg": atraso,
                "clientes": info["clientes"]
            }
            total_contas += len(info["clientes"])
            for c in info["clientes"]:
                val = c.get("zen", 0)
                if isinstance(val, int):
                    total_zen += val

    return {
        "total_maquinas": len(ativas),
        "total_contas": total_contas,
        "total_zen": total_zen,
        "maquinas": ativas
    }

@app.get("/", response_class=HTMLResponse)
def dashboard():
    return """
    <!DOCTYPE html>
    <html lang="pt-BR">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>ZenMonitor Cloud</title>
        <script src="https://cdn.tailwindcss.com"></script>
    </head>
    <body class="bg-slate-900 text-slate-100 min-h-screen p-6">
        <div class="max-w-6xl mx-auto">
            <header class="flex flex-col md:flex-row justify-between items-center mb-8 border-b border-slate-700 pb-4">
                <div>
                    <h1 class="text-2xl font-bold text-emerald-400">⚡ ZenMonitor Cloud</h1>
                    <p class="text-slate-400 text-sm">Monitoramento distribuído em tempo real</p>
                </div>
                <div class="flex gap-4 mt-4 md:mt-0">
                    <div class="bg-slate-800 px-4 py-2 rounded-lg border border-slate-700 text-center">
                        <span class="text-xs text-slate-400 block">PCs Online</span>
                        <span id="tot-pc" class="text-xl font-bold text-white">0</span>
                    </div>
                    <div class="bg-slate-800 px-4 py-2 rounded-lg border border-slate-700 text-center">
                        <span class="text-xs text-slate-400 block">Contas Ativas</span>
                        <span id="tot-acc" class="text-xl font-bold text-emerald-400">0</span>
                    </div>
                    <div class="bg-slate-800 px-4 py-2 rounded-lg border border-slate-700 text-center">
                        <span class="text-xs text-slate-400 block">Zen Total</span>
                        <span id="tot-zen" class="text-xl font-bold text-amber-400">0</span>
                    </div>
                </div>
            </header>
            <div id="grid-maquinas" class="grid grid-cols-1 md:grid-cols-2 gap-6"></div>
        </div>
        <script>
            const formatZen = (n) => n ? n.toLocaleString('pt-BR') : '0';
            async function atualizar() {
                try {
                    const res = await fetch('/api/v1/aggregate');
                    const data = await res.json();
                    document.getElementById('tot-pc').innerText = data.total_maquinas;
                    document.getElementById('tot-acc').innerText = data.total_contas;
                    document.getElementById('tot-zen').innerText = formatZen(data.total_zen);
                    const container = document.getElementById('grid-maquinas');
                    container.innerHTML = '';
                    if (data.total_maquinas === 0) {
                        container.innerHTML = '<div class="col-span-2 text-center py-12 bg-slate-800/50 rounded-xl border border-slate-800 text-slate-400">Nenhuma máquina reportando no momento. Inicie o agent_sender.py nos PCs locais.</div>';
                        return;
                    }
                    for (const [pc, info] of Object.entries(data.maquinas)) {
                        let linhas = '';
                        for (const c of info.clientes) {
                            linhas += `
                                <tr class="border-b border-slate-700/50 last:border-0">
                                    <td class="py-2 text-slate-400 text-sm">#${c.pid}</td>
                                    <td class="py-2 font-medium text-white">${c.name}</td>
                                    <td class="py-2 text-amber-400 font-mono font-semibold text-right">${formatZen(c.zen)}</td>
                                    <td class="py-2 text-right"><span class="px-2 py-0.5 text-xs rounded bg-emerald-500/20 text-emerald-300">${c.status || 'OK'}</span></td>
                                </tr>`;
                        }
                        container.innerHTML += `
                            <div class="bg-slate-800 rounded-xl p-5 border border-slate-700 shadow-lg">
                                <div class="flex justify-between items-center mb-4 pb-2 border-b border-slate-700">
                                    <h2 class="text-lg font-bold text-emerald-400 flex items-center gap-2">🖥️ ${pc}</h2>
                                    <span class="text-xs text-slate-400">há ${info.ultimo_envio_seg}s</span>
                                </div>
                                <table class="w-full text-left">
                                    <thead>
                                        <tr class="text-xs text-slate-400 border-b border-slate-700">
                                            <th class="pb-2">PID</th>
                                            <th class="pb-2">Personagem</th>
                                            <th class="pb-2 text-right">Zen</th>
                                            <th class="pb-2 text-right">Status</th>
                                        </tr>
                                    </thead>
                                    <tbody>${linhas}</tbody>
                                </table>
                            </div>`;
                    }
                } catch (e) { console.error(e); }
            }
            atualizar();
            setInterval(atualizar, 3000);
        </script>
    </body>
    </html>
    """
