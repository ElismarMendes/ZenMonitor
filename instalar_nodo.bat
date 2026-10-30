@echo off
chcp 65001 >nul
title Instalador ZenMonitor Cloud - Auto-Reparo
echo ===================================================
echo   INSTALANDO E VERIFICANDO ZENMONITOR CLOUD
echo ===================================================

if not exist "C:\Mu" mkdir "C:\Mu"
cd /d "C:\Mu"

if not exist "C:\Mu\ZenCloud\.git" (
    echo [1/5] Baixando projeto do GitHub...
    git clone https://github.com/ElismarMendes/ZenMonitor.git ZenCloud
) else (
    echo [1/5] Sincronizando arquivos com o GitHub...
    cd /d "C:\Mu\ZenCloud"
    git fetch --all
    git reset --hard origin/main
)

cd /d "C:\Mu\ZenCloud"

echo [2/5] Verificando o Tesseract-OCR no sistema...
if exist "C:\Program Files\Tesseract-OCR\tesseract.exe" (
    echo [OK] Tesseract-OCR ja esta instalado!
) else (
    echo [!] Baixando e instalando Tesseract-OCR automaticamente...
    powershell -Command "Invoke-WebRequest -Uri 'https://digi.bib.uni-mannheim.de/tesseract/tesseract-ocr-w64-setup-5.3.3.20231005.exe' -OutFile 'tesseract_installer.exe'"
    start /wait "" tesseract_installer.exe /S /D=C:\Program Files\Tesseract-OCR
    if exist "tesseract_installer.exe" del tesseract_installer.exe
)

:: 3. Verifica o executavel real da .venv (se apagou ou corrompeu, recria do zero)
echo [3/5] Verificando ambiente virtual (.venv)...
if not exist ".venv\Scripts\python.exe" (
    echo [!] Ambiente .venv ausente ou incompleto. Recriando...
    if exist ".venv" rmdir /s /q ".venv"
    python -m venv .venv
)

:: 4. Instala as dependencias usando o pip de dentro da .venv
echo [4/5] Verificando e instalando dependencias do requirements.txt...
if exist ".venv\Scripts\python.exe" (
    .venv\Scripts\python.exe -m pip install --upgrade pip
    .venv\Scripts\python.exe -m pip install -r requirements.txt
) else (
    echo [ERRO CRITICO] O Python nao conseguiu criar a pasta .venv! Verifique se o Python esta no PATH do Windows.
    pause
    exit /b
)

:: 5. Cria o iniciar_agente_local.bat tambem com auto-reparo caso a .venv seja apagada no futuro
(
echo @echo off
echo chcp 65001 ^>nul
echo title Agente ZenMonitor - %COMPUTERNAME%
echo cd /d "C:\Mu\ZenCloud"
echo if not exist ".venv\Scripts\python.exe" ^(
echo     echo [!] Pasta .venv nao encontrada. Recriando ambiente e instalando pacotes...
echo     python -m venv .venv
echo     .venv\Scripts\python.exe -m pip install --upgrade pip
echo     .venv\Scripts\python.exe -m pip install -r requirements.txt
echo ^)
echo .venv\Scripts\python.exe agent_sender.py
echo echo.
echo echo O agente foi encerrado.
echo pause
) > iniciar_agente_local.bat

echo.
echo ===================================================
echo   [5/5] TUDO PRONTO! INICIANDO O AGENTE LOCAL...
echo ===================================================
.venv\Scripts\python.exe agent_sender.py
pause