class MonitorService:
    """
    Substitua este arquivo pelo seu monitor_service.py real 
    que chama o core/memory.py, core/windows.py e core/ocr.py.
    """
    def coletar_contas(self):
        # Retorno de teste inicial para validar a conexao com a VPS de imediato
        return [
            {"pid": 1024, "name": "CharTeste_1", "resolucao": "800x600", "zen": 154000000, "status": "Online"},
            {"pid": 2048, "name": "CharTeste_2", "resolucao": "800x600", "zen": 89500000, "status": "Online"}
        ]
