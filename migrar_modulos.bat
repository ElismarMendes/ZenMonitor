@echo off
chcp 65001 >nul
echo Copiando modulos reais do projeto antigo...

xcopy /Y /E "C:\Mu\zen_api\core\*" "C:\Mu\ZenCloud\local_agent\core\"
xcopy /Y /E "C:\Mu\zen_api\services\*" "C:\Mu\ZenCloud\local_agent\services\"

echo Concluido!
pause