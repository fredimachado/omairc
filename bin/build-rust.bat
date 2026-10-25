@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0.."
set "ROOT=%CD%"
set "BUILD_DIR=%ROOT%\build"
set "REL=%BUILD_DIR%\release"
set "POC=%ROOT%\poc"

where cargo >nul 2>&1
if errorlevel 1 (
  echo cargo not found. Install Rust and put cargo on PATH. >&2
  exit /b 1
)

call :ensure_msvc
if errorlevel 1 exit /b 1

rem Run from poc so poc\.cargo\config.toml applies for static CRT rustflags.
rem Cargo only walks .cargo from the current directory, not the manifest path.
echo Building Rust product client - release, locked, features=product
pushd "!POC!"
if errorlevel 1 exit /b 1
cargo build --release --locked --features product
if errorlevel 1 (
  popd
  exit /b 1
)
popd

set "EXE=!POC!\target\release\omairc.exe"
if not exist "!EXE!" (
  echo Expected !EXE! after cargo build. >&2
  exit /b 1
)

call :check_static_crt "!EXE!"
if errorlevel 1 exit /b 1

if exist "!REL!" rmdir /s /q "!REL!"
mkdir "!REL!"
copy /Y "!EXE!" "!REL!\omairc.exe" >nul
if errorlevel 1 exit /b 1
copy /Y "!ROOT!\LICENSE" "!REL!\LICENSE" >nul
copy /Y "!ROOT!\fonts\OFL.txt" "!REL!\OFL.txt" >nul
copy /Y "!ROOT!\src\irc\COPYING" "!REL!\COPYING-LGPL" >nul
if not exist "!REL!\LICENSE" (
  echo Failed to stage LICENSE into !REL!. >&2
  exit /b 1
)
if not exist "!REL!\OFL.txt" (
  echo Failed to stage OFL.txt into !REL!. >&2
  exit /b 1
)
if not exist "!REL!\COPYING-LGPL" (
  echo Failed to stage COPYING-LGPL into !REL!. >&2
  exit /b 1
)

echo Built !REL!\omairc.exe
exit /b 0

:check_static_crt
set "TARGET=%~1"
where dumpbin >nul 2>&1
if errorlevel 1 (
  echo warning: dumpbin not on PATH; skipping static CRT check. >&2
  exit /b 0
)
set "DEPS=%TEMP%\omairc-dumpbin-dependents.txt"
dumpbin /dependents "!TARGET!" > "!DEPS!"
if errorlevel 1 (
  echo dumpbin /dependents failed for !TARGET!. >&2
  exit /b 1
)
findstr /i /c:"VCRUNTIME" /c:"MSVCP" "!DEPS!" >nul
if not errorlevel 1 (
  echo Static CRT check failed: !TARGET! links the MSVC CRT DLL. >&2
  type "!DEPS!" >&2
  exit /b 1
)
echo Static CRT ok - no VCRUNTIME / MSVCP dependents
exit /b 0

:ensure_msvc
where cl >nul 2>&1
if not errorlevel 1 exit /b 0

set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "!VSWHERE!" set "VSWHERE=%ProgramFiles%\Microsoft Visual Studio\Installer\vswhere.exe"
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
where cl >nul 2>&1
if errorlevel 1 (
  echo cl.exe still not on PATH after vcvars64.bat. >&2
  exit /b 1
)
exit /b 0
