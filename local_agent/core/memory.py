import ctypes
from ctypes import wintypes
import struct
import logging

PROCESS_VM_READ = 0x0010
PROCESS_QUERY_INFORMATION = 0x0400
MEM_COMMIT = 0x1000
PAGE_READWRITE = 0x04
PAGE_EXECUTE_READWRITE = 0x40

class MEMORY_BASIC_INFORMATION(ctypes.Structure):
    _fields_ = [
        ("BaseAddress", ctypes.c_size_t),
        ("AllocationBase", ctypes.c_size_t),
        ("AllocationProtect", wintypes.DWORD),
        ("RegionSize", ctypes.c_size_t),
        ("State", wintypes.DWORD),
        ("Protect", wintypes.DWORD),
        ("Type", ctypes.c_size_t),
    ]

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
kernel32.OpenProcess.restype = wintypes.HANDLE
kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
kernel32.CloseHandle.restype = wintypes.BOOL

def ler_multiplos_enderecos(pid: int, enderecos: list) -> dict:
    resultado = {}
    h_proc = kernel32.OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, False, pid)
    if not h_proc:
        return resultado
    try:
        buffer = ctypes.create_string_buffer(4)
        lidos = ctypes.c_size_t(0)
        for end in enderecos:
            if kernel32.ReadProcessMemory(h_proc, ctypes.c_void_p(end), buffer, 4, ctypes.byref(lidos)):
                if lidos.value == 4:
                    val = struct.unpack("<I", buffer.raw)[0]
                    resultado[end] = val
    finally:
        kernel32.CloseHandle(h_proc)
    return resultado

def buscar_valor_4bytes(pid: int, valor_zen: int, candidatos_atuais: set = None) -> set:
    if not (0 <= valor_zen <= 0xFFFFFFFF):
        return set()

    h_proc = kernel32.OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, False, pid)
    if not h_proc:
        return set()

    novos_candidatos = set()
    bytes_alvo = struct.pack("<I", valor_zen)

    try:
        if candidatos_atuais is not None:
            buffer = ctypes.create_string_buffer(4)
            lidos = ctypes.c_size_t(0)
            for end in candidatos_atuais:
                if kernel32.ReadProcessMemory(h_proc, ctypes.c_void_p(end), buffer, 4, ctypes.byref(lidos)):
                    if lidos.value == 4 and buffer.raw == bytes_alvo:
                        novos_candidatos.add(end)
            return novos_candidatos

        mbi = MEMORY_BASIC_INFORMATION()
        endereco = 0
        limite_max = 0x7FFFFFFF0000 if ctypes.sizeof(ctypes.c_void_p) == 8 else 0x7FFFFFFF

        while endereco < limite_max:
            if not kernel32.VirtualQueryEx(h_proc, ctypes.c_void_p(endereco), ctypes.byref(mbi), ctypes.sizeof(mbi)):
                break

            if mbi.State == MEM_COMMIT and mbi.Protect in (PAGE_READWRITE, PAGE_EXECUTE_READWRITE):
                if 0 < mbi.RegionSize < 20 * 1024 * 1024:
                    buf = ctypes.create_string_buffer(mbi.RegionSize)
                    lidos = ctypes.c_size_t(0)
                    if kernel32.ReadProcessMemory(h_proc, ctypes.c_void_p(mbi.BaseAddress), buf, mbi.RegionSize, ctypes.byref(lidos)):
                        dados = buf.raw[:lidos.value]
                        pos = dados.find(bytes_alvo)
                        while pos != -1:
                            novos_candidatos.add(mbi.BaseAddress + pos)
                            pos = dados.find(bytes_alvo, pos + 4)

            if mbi.RegionSize == 0:
                break
            endereco = mbi.BaseAddress + mbi.RegionSize
    finally:
        kernel32.CloseHandle(h_proc)

    return novos_candidatos