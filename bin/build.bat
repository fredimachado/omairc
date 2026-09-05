@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0.."
set "ROOT=%CD%"
set "BUILD_DIR=%ROOT%\build"

call :find_qmake
if errorlevel 1 exit /b 1

for %%I in ("!QMAKE!") do set "QTBIN=%%~dpI"
if "!QTBIN:~-1!"=="\" set "QTBIN=!QTBIN:~0,-1!"
set "PATH=!QTBIN!;!PATH!"

for /f "delims=" %%I in ('"!QMAKE!" -query QMAKE_SPEC') do set "QSPEC=%%I"
for /f "delims=" %%I in ('"!QMAKE!" -query QT_VERSION') do set "QT_VER=%%I"
echo Using Qt !QT_VER! (!QSPEC!) at !QTBIN!

if /i "!QSPEC!"=="win32-msvc" (
  call :ensure_msvc
  if errorlevel 1 exit /b 1
)

if /i "!QSPEC!"=="win32-g++" (
  call :ensure_mingw
  if errorlevel 1 exit /b 1
)

if not exist "!BUILD_DIR!" mkdir "!BUILD_DIR!"
cd /d "!BUILD_DIR!"

"!QMAKE!" "!ROOT!\omairc.pro"
if errorlevel 1 exit /b 1

call :run_make
if errorlevel 1 exit /b 1

set "EXE=!BUILD_DIR!\release\omairc.exe"
if not exist "!EXE!" (
  echo Expected !EXE! after the build. >&2
  exit /b 1
)

if exist "!QTBIN!\windeployqt.exe" (
  "!QTBIN!\windeployqt.exe" --qmldir "!ROOT!\src" "!EXE!"
  if errorlevel 1 exit /b 1
)

echo Built !EXE!
exit /b 0

:find_qmake
if defined QMAKE (
  if exist "!QMAKE!" exit /b 0
  echo QMAKE is set but not found: !QMAKE! >&2
  exit /b 1
)

where qmake >nul 2>&1
if not errorlevel 1 (
  for /f "delims=" %%I in ('where qmake') do (
    set "CAND=%%I"
    goto :check_path_qmake
  )
)

:check_path_qmake
if defined CAND (
  set "VER="
  for /f "delims=" %%I in ('"!CAND!" -query QT_VERSION 2^>nul') do set "VER=%%I"
  if "!VER:~0,2!"=="6." (
    set "QMAKE=!CAND!"
    exit /b 0
  )
)

if exist "C:\Qt" (
  for /d %%V in ("C:\Qt\6.*") do (
    for /d %%K in ("%%V\msvc*_64") do (
      if exist "%%K\bin\qmake.exe" set "QMAKE=%%K\bin\qmake.exe"
    )
  )
  if defined QMAKE exit /b 0
  for /d %%V in ("C:\Qt\6.*") do (
    if exist "%%V\mingw_64\bin\qmake.exe" set "QMAKE=%%V\mingw_64\bin\qmake.exe"
  )
  if defined QMAKE exit /b 0
)

echo A Qt 6 qmake is required to build omairc. >&2
echo Install a Qt 6 kit, add qmake to PATH, or set QMAKE to qmake.exe. >&2
exit /b 1

:ensure_msvc
where cl >nul 2>&1
if not errorlevel 1 (
  where nmake >nul 2>&1
  if not errorlevel 1 exit /b 0
)

set "VSWHERE=!ProgramFiles(x86)!\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "!VSWHERE!" (
  echo vswhere.exe not found. Open an x64 Native Tools Command Prompt and rerun. >&2
  exit /b 1
)

set "VSINSTALL="
for /f "delims=" %%I in ('"!VSWHERE!" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath') do set "VSINSTALL=%%I"
if not defined VSINSTALL (
  echo No Visual Studio C++ x64 toolchain found. >&2
  exit /b 1
)

set "VCVARS=!VSINSTALL!\VC\Auxiliary\Build\vcvars64.bat"
if not exist "!VCVARS!" (
  echo vcvars64.bat not found at !VCVARS! >&2
  exit /b 1
)

call "!VCVARS!"
if errorlevel 1 exit /b 1
set "PATH=!QTBIN!;!PATH!"
where cl >nul 2>&1
if errorlevel 1 (
  echo cl.exe still not on PATH after vcvars64.bat. >&2
  exit /b 1
)
exit /b 0

:ensure_mingw
where mingw32-make >nul 2>&1
if not errorlevel 1 exit /b 0
if exist "C:\Qt\Tools" (
  for /d %%M in ("C:\Qt\Tools\mingw*") do (
    if exist "%%M\bin\mingw32-make.exe" set "MINGWBIN=%%M\bin"
  )
)
if not defined MINGWBIN (
  echo mingw32-make not found. Add the MinGW compiler bin directory to PATH. >&2
  exit /b 1
)
set "PATH=!MINGWBIN!;!PATH!"
exit /b 0

:run_make
if /i "!QSPEC!"=="win32-g++" (
  mingw32-make -j %NUMBER_OF_PROCESSORS%
  exit /b !errorlevel!
)

set "JOM="
where jom >nul 2>&1
if not errorlevel 1 (
  for /f "delims=" %%I in ('where jom') do (
    set "JOM=%%I"
    goto :have_make
  )
)
if exist "C:\Qt\Tools\QtCreator\bin\jom\jom.exe" set "JOM=C:\Qt\Tools\QtCreator\bin\jom\jom.exe"

:have_make
if defined JOM (
  "!JOM!"
  exit /b !errorlevel!
)
nmake
exit /b !errorlevel!
