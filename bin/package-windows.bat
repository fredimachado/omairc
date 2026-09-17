@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0.."
set "ROOT=%CD%"

set "EXE=%ROOT%\build\release\omairc.exe"
if not exist "!EXE!" (
  echo Expected !EXE!. Run bin\build.bat first. >&2
  exit /b 1
)

set "VERSION="
if defined OMAIRC_VERSION (
  set "VERSION=!OMAIRC_VERSION!"
) else (
  for /f "usebackq tokens=2 delims==" %%I in (`findstr /b /c:"VERSION " "%ROOT%\version.pri"`) do set "VERSION=%%I"
)
set "VERSION=!VERSION: =!"
if not defined VERSION (
  echo version.pri must set VERSION >&2
  exit /b 1
)

call :find_iscc
if errorlevel 1 exit /b 1

if not exist "%ROOT%\dist" mkdir "%ROOT%\dist"

echo Compiling Omairc !VERSION! setup with "!ISCC!"
"!ISCC!" /Q /DMyAppVersion=!VERSION! "%ROOT%\packaging\windows\omairc.iss"
if errorlevel 1 exit /b 1

set "SETUP=%ROOT%\dist\omairc-!VERSION!-windows-x64-setup.exe"
if not exist "!SETUP!" (
  echo Expected !SETUP! after ISCC. >&2
  exit /b 1
)

echo Built !SETUP!
exit /b 0

:find_iscc
if defined ISCC (
  if exist "!ISCC!" exit /b 0
  echo ISCC is set but not found: !ISCC! >&2
  exit /b 1
)

where iscc >nul 2>&1
if not errorlevel 1 (
  for /f "delims=" %%I in ('where iscc') do (
    set "ISCC=%%I"
    exit /b 0
  )
)

for %%P in (
  "%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe"
  "%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe"
  "%ProgramFiles%\Inno Setup 6\ISCC.exe"
) do (
  if exist %%P (
    set "ISCC=%%~P"
    exit /b 0
  )
)

echo Inno Setup 6 is required to compile the Windows installer. >&2
echo Install it, add ISCC.exe to PATH, or set ISCC to ISCC.exe. >&2
exit /b 1
