from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import Dict, Any, List
import time

app = FastAPI(
    title="Zen Central VPS Server",
    version="1.0",
    description="Servidor central na VPS para agregar status de múltiplas máquinas."
)

# Dicionário em memória para armazenar os dados enviados por máquina
# Estrutura: { "nome_maquina": { "timestamp": X, "clientes": [...] } }
DADOS_MAQUINAS: Dict[str, Any] = {}

class PayloadMaquina(BaseModel, extra="allow"):
    machine_id: str
    clientes: List[Dict[str, Any]]

@app.post("/api/v1/report", summary="Recebe o relatório de contas de um PC local")
def receber_relatorio(payload: PayloadMaquina):
    """
    Cada computador local envia seu JSON de contas periodicamente para cá.
    """
    machine_id = payload.machine_id
    DADOS_MAQUINAS[machine_id] = {
        "timestamp": time.time(),
        "clientes": payload.clientes
    }
    return {"status": "success", "machine_id": machine_id, "recebidos": len(payload.clientes)}

@app.get("/api/v1/aggregate", summary="Retorna todas as contas de todas as máquinas para o Front-end")
def obter_dados_consolidados():
    """
    O painel central vai consultar este endpoint para ver tudo em uma única tela.
    Remove automaticamente máquinas que pararam de enviar dados há mais de 30 segundos.
    """
    agora = time.time()
    maquinas_ativas = {}
    
    for machine_id, dados in DADOS_MAQUINAS.items():
        # Se a máquina não enviar ping/dados em 30 segundos, consideram-se offline
        if agora - dados["timestamp"] < 30:
            maquinas_ativas[machine_id] = dados["clientes"]
            
    return {
        "total_maquinas": len(maquinas_ativas),
        "maquinas": maquinas_ativas
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run("vps_main:app", host="0.0.0.0", port=8000, reload=True)