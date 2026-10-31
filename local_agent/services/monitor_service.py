# -*- coding: utf-8 -*-
import os
import sys
from concurrent.futures import ThreadPoolExecutor
import cv2

PASTA_SERVICES = os.path.dirname(os.path.abspath(__file__))
PASTA_AGENT = os.path.dirname(PASTA_SERVICES)
if PASTA_AGENT not in sys.path:
    sys.path.insert(0, PASTA_AGENT)

from core.windows import obter_clientes_mainmu, capturar_tela_janela_fundo
from core.ocr import ler_texto_imagem
from core.memory import ler_multiplos_enderecos, buscar_valor_4bytes
from core.calibrator import (
    obter_box_inteligente,
    obter_enderecos_ram_salvos,
    salvar_enderecos_ram_conta,
)
from core.logger import log_capture, log_ocr, log_memory


class MonitorService:
    def __init__(self):
        self.ultimo_zen_valido = {}
        self.estado_memoria = {}
        self.executor = ThreadPoolExecutor(max_workers=4)

    def listar_contas_ativas(self):
        return obter_clientes_mainmu()

    def _ler_ocr_janela(self, pid, char_name, hwnd, resolucao):
        if resolucao == "Minimizado":
            return None

        rel_box = obter_box_inteligente(hwnd, resolucao, char_name)
        if not rel_box:
            return None

        # Tenta a captura de fundo padrao. Se falhar em segundo plano, tenta forcar captura direta
        img = capturar_tela_janela_fundo(hwnd, rel_box)
        if img is None or img.size == 0 or cv2.mean(img)[0] < 2.0:
            img = capturar_tela_janela_fundo(hwnd, rel_box, forcar_tela_direta=True)

        if img is None or img.size == 0:
            log_capture(pid, char_name, "BitBlt/ImageGrab", "Vazio ou Inacessivel em segundo plano")
            return None

        valor = ler_texto_imagem(img)
        if valor is not None and 1000 <= valor <= 2000000000:
            log_ocr(pid, char_name, "OK", valor)
            return valor

        # Salva imagem de diagnostico se falhar para inspecionar o motivo do OCR
        try:
            cv2.imwrite(f"falha_ocr_{char_name}.png", img)
        except Exception:
            pass

        log_ocr(pid, char_name, "FALHA OCR", None)
        return None

    def obter_zen_conta(self, pid, info_janela=None):
        pid = int(pid)
        if not info_janela:
            clientes = self.listar_contas_ativas()
            info_janela = clientes.get(pid)

        if not info_janela:
            return {"zen": 0, "status": "Janela fechada"}

        hwnd = info_janela.get("hwnd")
        char_name = info_janela.get("name", f"PID:{pid}")
        resolucao = info_janela.get("resolucao", "1200x900")
        zen_cache = self.ultimo_zen_valido.get(pid, 0)

        estado = self.estado_memoria.get(char_name)
        if not estado or estado["pid"] != pid:
            enderecos_salvos = obter_enderecos_ram_salvos(char_name, pid)
            estado = {
                "pid": pid,
                "candidatos": set(),
                "ultimo_zen": None,
                "leitura_semente": None,
                "falhas_filtro": 0,
                "enderecos_travados": enderecos_salvos,
            }
            self.estado_memoria[char_name] = estado

        # LEITURA DIRETA DA MEMORIA RAM (Caso ja tenha travado o endereco)
        if estado["enderecos_travados"]:
            leituras = ler_multiplos_enderecos(pid, estado["enderecos_travados"])
            valores_validos = [v for v in leituras.values() if 1000 <= v <= 2000000000]
            if valores_validos:
                zen_ram = max(valores_validos)
                self.ultimo_zen_valido[pid] = zen_ram
                end_principal = hex(estado["enderecos_travados"][0])
                log_memory(pid, char_name, "LOCKED-RAM", f"Endereco {end_principal} ativo", zen_ram)
                return {
                    "zen": zen_ram,
                    "status": f"RAM [{end_principal}] (Pode fechar V)",
                }
            else:
                log_memory(pid, char_name, "RESET", "Enderecos antigos invalidados. Reiniciando scan...")
                estado["enderecos_travados"] = []

        # OCR + VARREDURA NA RAM
        zen_ocr = self._ler_ocr_janela(pid, char_name, hwnd, resolucao)

        if not estado["candidatos"]:
            if zen_ocr is not None:
                semente = estado.get("leitura_semente")
                if semente is None or abs(zen_ocr - semente) > 20000000:
                    estado["leitura_semente"] = zen_ocr
                    self.ultimo_zen_valido[pid] = zen_ocr
                    return {"zen": zen_ocr, "status": "Confirmando OCR para 1o Scan..."}

                cands = buscar_valor_4bytes(pid, zen_ocr, None)
                if cands:
                    estado["candidatos"] = cands
                    estado["ultimo_zen"] = zen_ocr
                    estado["falhas_filtro"] = 0
                    self.ultimo_zen_valido[pid] = zen_ocr
                    log_memory(pid, char_name, "1o SCAN", f"{len(cands)} enderecos encontrados", zen_ocr)

                    if len(cands) <= 10:
                        estado["enderecos_travados"] = sorted(cands)
                        salvar_enderecos_ram_conta(char_name, pid, estado["enderecos_travados"])
                        end_hex = hex(estado["enderecos_travados"][0])
                        return {"zen": zen_ocr, "status": f"RAM [{end_hex}] (Pode fechar V)"}

                    return {"zen": zen_ocr, "status": f"1o Scan ({len(cands)} cand.) - Cate Zen!"}
                else:
                    self.ultimo_zen_valido[pid] = zen_ocr
                    return {"zen": zen_ocr, "status": "OCR OK (Sem acesso RAM)"}

            return {"zen": zen_cache, "status": "Abra o 'V' para 1o Scan"}

        if zen_ocr is not None and zen_ocr != estado["ultimo_zen"]:
            if abs(zen_ocr - estado["ultimo_zen"]) <= 50000000:
                novos_cands = buscar_valor_4bytes(pid, zen_ocr, estado["candidatos"])
                if len(novos_cands) > 0:
                    estado["candidatos"] = novos_cands
                    estado["ultimo_zen"] = zen_ocr
                    estado["falhas_filtro"] = 0
                    self.ultimo_zen_valido[pid] = zen_ocr

                    if len(novos_cands) <= 10:
                        estado["enderecos_travados"] = sorted(novos_cands)
                        salvar_enderecos_ram_conta(char_name, pid, estado["enderecos_travados"])
                        end_hex = hex(estado["enderecos_travados"][0])
                        log_memory(pid, char_name, "TRAVADO!", f"Endereco {end_hex} salvo!", zen_ocr)
                        return {"zen": zen_ocr, "status": f"RAM [{end_hex}] (Pode fechar V)"}
                else:
                    estado["falhas_filtro"] += 1
                    if estado["falhas_filtro"] >= 4:
                        estado["candidatos"] = set()
                        estado["leitura_semente"] = None

        amostra_ram = ler_multiplos_enderecos(pid, list(estado["candidatos"])[:15])
        valores_ram = [v for v in amostra_ram.values() if 1000 <= v <= 2000000000]
        zen_exibir = zen_ocr if zen_ocr is not None else (max(valores_ram) if valores_ram else zen_cache)
        if zen_exibir > 0:
            self.ultimo_zen_valido[pid] = zen_exibir

        return {
            "zen": zen_exibir,
            "status": f"Filtrando RAM ({len(estado['candidatos'])} cand.) - Cate Zen!",
        }

    def processar_todas_contas_paralelo(self, clientes):
        futures = {}
        for pid, info in clientes.items():
            futures[pid] = self.executor.submit(self.obter_zen_conta, pid, info)

        clientes_payload = []
        for pid, future in futures.items():
            info = clientes[pid]
            res_zen = future.result()
            clientes_payload.append({
                "pid": int(pid),
                "name": str(info.get("name", f"PID:{pid}")),
                "resolucao": info.get("resolucao", "N/A"),
                "zen": res_zen.get("zen", 0),
                "status": res_zen.get("status", "OK"),
            })
        return clientes_payload