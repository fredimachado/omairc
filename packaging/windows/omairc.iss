; Inno Setup 6 script for the Windows tree produced by bin\build.bat.
; Compile through bin\package-windows.bat so MyAppVersion comes from version.pri.
;
; Per-user is the default (no UAC). {autopf} is %LOCALAPPDATA%\Programs\Omairc
; then, or Program Files when the user picks "all users".

#ifndef MyAppVersion
  #error MyAppVersion must be defined (run bin\package-windows.bat)
#endif

#define MyAppName "Omairc"
#define MyAppExeName "omairc.exe"

[Setup]
AppId={{37400F83-8332-4043-8DFF-1F69E51F6555}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppName}
AppPublisherURL=https://omairc.app
AppSupportURL=https://github.com/fredimachado/omairc/issues
AppUpdatesURL=https://github.com/fredimachado/omairc/releases
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
ChangesEnvironment=yes
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
CloseApplications=yes
LicenseFile=..\..\LICENSE
SetupIconFile=..\..\data\icons\omairc.ico
OutputDir=..\..\dist
OutputBaseFilename=omairc-{#MyAppVersion}-windows-x64-setup

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "..\..\build\release\*"; DestDir: "{app}"; \
  Flags: ignoreversion recursesubdirs createallsubdirs; \
  Excludes: "*.obj,*.pdb,*.res,*.ilk,moc_*.cpp,moc_predefs.h,qrc_*.cpp"
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; \
  Comment: "A dead-simple IRC client for Omarchy"

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; \
  Flags: nowait postinstall skipifsilent runasoriginaluser

[Code]
function EnvRootKey: Integer;
begin
  if IsAdminInstallMode then
    Result := HKLM
  else
    Result := HKCU;
end;

function EnvSubKey: String;
begin
  if IsAdminInstallMode then
    Result := 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
  else
    Result := 'Environment';
end;

procedure SplitSemicolon(const S: String; var Parts: TArrayOfString);
var
  StartPos, I, Count: Integer;
begin
  Count := 0;
  SetArrayLength(Parts, 0);
  StartPos := 1;
  for I := 1 to Length(S) do
  begin
    if S[I] = ';' then
    begin
      SetArrayLength(Parts, Count + 1);
      Parts[Count] := Copy(S, StartPos, I - StartPos);
      Inc(Count);
      StartPos := I + 1;
    end;
  end;
  SetArrayLength(Parts, Count + 1);
  Parts[Count] := Copy(S, StartPos, Length(S) - StartPos + 1);
end;

function JoinNonEmpty(const Parts: TArrayOfString): String;
var
  I: Integer;
begin
  Result := '';
  for I := 0 to GetArrayLength(Parts) - 1 do
  begin
    if Parts[I] = '' then
      Continue;
    if Result = '' then
      Result := Parts[I]
    else
      Result := Result + ';' + Parts[I];
  end;
end;

function SameDir(const Left, Right: String): Boolean;
begin
  Result := CompareText(RemoveBackslash(Left), RemoveBackslash(Right)) = 0;
end;

function PathListHasDir(const PathList, Dir: String): Boolean;
var
  Parts: TArrayOfString;
  I: Integer;
begin
  SplitSemicolon(PathList, Parts);
  Result := False;
  for I := 0 to GetArrayLength(Parts) - 1 do
  begin
    if (Parts[I] <> '') and SameDir(Parts[I], Dir) then
    begin
      Result := True;
      Exit;
    end;
  end;
end;

procedure AddToPath(const Dir: String);
var
  OrigPath: String;
  Parts: TArrayOfString;
begin
  if Dir = '' then
    Exit;
  if not RegQueryStringValue(EnvRootKey, EnvSubKey, 'Path', OrigPath) then
    OrigPath := '';
  if PathListHasDir(OrigPath, Dir) then
    Exit;
  SplitSemicolon(OrigPath, Parts);
  SetArrayLength(Parts, GetArrayLength(Parts) + 1);
  Parts[GetArrayLength(Parts) - 1] := Dir;
  if not RegWriteExpandStringValue(EnvRootKey, EnvSubKey, 'Path', JoinNonEmpty(Parts)) then
    Log('Could not add Omairc to PATH');
end;

procedure RemoveFromPath(const Dir: String);
var
  OrigPath: String;
  Parts, Kept: TArrayOfString;
  I, Count: Integer;
begin
  if Dir = '' then
    Exit;
  if not RegQueryStringValue(EnvRootKey, EnvSubKey, 'Path', OrigPath) then
    Exit;
  if not PathListHasDir(OrigPath, Dir) then
    Exit;
  SplitSemicolon(OrigPath, Parts);
  Count := 0;
  SetArrayLength(Kept, 0);
  for I := 0 to GetArrayLength(Parts) - 1 do
  begin
    if (Parts[I] = '') or SameDir(Parts[I], Dir) then
      Continue;
    SetArrayLength(Kept, Count + 1);
    Kept[Count] := Parts[I];
    Inc(Count);
  end;
  if not RegWriteExpandStringValue(EnvRootKey, EnvSubKey, 'Path', JoinNonEmpty(Kept)) then
    Log('Could not remove Omairc from PATH');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    AddToPath(ExpandConstant('{app}'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(ExpandConstant('{app}'));
end;
