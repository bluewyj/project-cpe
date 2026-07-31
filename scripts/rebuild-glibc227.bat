@echo off
call "%~dp0build-windows.bat"
if errorlevel 1 exit /b 1
powershell -ExecutionPolicy Bypass -File "%~dp0pack-ota.ps1"
exit /b %ERRORLEVEL%
