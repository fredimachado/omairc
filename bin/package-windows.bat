@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0.."
set "ROOT=%CD%"

set "EXE=%ROOT%\build\release\omairc.exe"
if not exist "!EXE!" (
  echo Expected !EXE!. Run bin\build.bat first. >&2
  exit /b 1
)
if not exist "%ROOT%\build\release\msvcp140.dll" (
  echo msvcp140.dll is missing next to omairc.exe. >&2
  echo The installer needs the MSVC CRT DLLs. Rebuild from an x64 Native Tools >&2
  echo prompt so VCToolsRedistDir is set, then rerun bin\package-windows.bat. >&2
  exit /b 1
)

set "DISPLAY_VERSION="
if defined OMAIRC_DISPLAY_VERSION (
  set "DISPLAY_VERSION=!OMAIRC_DISPLAY_VERSION!"
) else if defined OMAIRC_BUILD_VERSION (
  set "DISPLAY_VERSION=!OMAIRC_BUILD_VERSION!"
) else if defined OMAIRC_VERSION (
  set "DISPLAY_VERSION=!OMAIRC_VERSION!"
) else (
  for /f "usebackq tokens=2 delims==" %%I in (`findstr /b /c:"VERSION " "%ROOT%\version.pri"`) do set "DISPLAY_VERSION=%%I"
)
set "DISPLAY_VERSION=!DISPLAY_VERSION: =!"
if not defined DISPLAY_VERSION (
  echo version.pri must set VERSION >&2
  exit /b 1
)

set "ARTIFACT_VERSION=!DISPLAY_VERSION!"
if defined OMAIRC_ARTIFACT_VERSION (
  set "ARTIFACT_VERSION=!OMAIRC_ARTIFACT_VERSION!"
)

call :find_iscc
if errorlevel 1 exit /b 1

if not exist "%ROOT%\dist" mkdir "%ROOT%\dist"

set "STAGE=%ROOT%\dist\omairc-windows-stage"
if exist "!STAGE!" rmdir /s /q "!STAGE!"
mkdir "!STAGE!"
xcopy /E /I /Q /Y "%ROOT%\build\release\*" "!STAGE!\" >nul
if errorlevel 1 (
  echo Failed to stage build\release for the installer. >&2
  exit /b 1
)
del /q "!STAGE!\*.obj" "!STAGE!\*.pdb" "!STAGE!\*.res" "!STAGE!\*.ilk" >nul 2>&1
del /q "!STAGE!\moc_*.cpp" "!STAGE!\moc_predefs.h" "!STAGE!\qrc_*.cpp" >nul 2>&1
del /q "!STAGE!\vc_redist*.exe" >nul 2>&1

echo Compiling Omairc !DISPLAY_VERSION! setup with "!ISCC!"
"!ISCC!" /Q /DMyAppVersion=!DISPLAY_VERSION! /DMyArtifactVersion=!ARTIFACT_VERSION! /DMyAppSource="..\..\dist\omairc-windows-stage" "%ROOT%\packaging\windows\omairc.iss"
if errorlevel 1 exit /b 1
if exist "!STAGE!" rmdir /s /q "!STAGE!"

set "SETUP=%ROOT%\dist\omairc-!ARTIFACT_VERSION!-windows-x64-setup.exe"
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

echo Inno Setup 6.1 or later is required to compile the Windows installer. >&2
echo Install it, add ISCC.exe to PATH, or set ISCC to ISCC.exe. >&2
exit /b 1
