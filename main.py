from fastapi import FastAPI, HTTPException
from services.monitor_service import MonitorService

app = FastAPI(
    title="Zen Monitor API Modular",
    version="2.0",
    description="API profissional para automação e monitoramento de jogos."
)

monitor = MonitorService()

@app.get("/api/v1/clientes", summary="Lista todas as instâncias ativas do jogo")
def get_clientes():
    clientes = monitor.listar_contas_ativas()
    resultado = [
        {"pid": pid, "name": info["name"], "titulo": info["titulo"], "resolucao": info["resolucao"]}
        for pid, info in clientes.items()
    ]
    return {"total": len(resultado), "clientes": resultado}

@app.get("/api/v1/zen/{pid}", summary="Obtém o Zen de uma conta específica")
def get_zen(pid: int):
    resultado = monitor.obter_zen_conta(pid)
    if "error" in resultado:
        raise HTTPException(status_code=404, detail=resultado["error"])
    return resultado

# Espaço reservado para futuras funções de helper (ex: @app.post("/api/v1/helper/..."))

if __name__ == "__main__":
    import uvicorn
    uvicorn.run("main:app", host="127.0.0.1", port=8000, reload=True)