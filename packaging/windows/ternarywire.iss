; Inno Setup script for the Windows installer (TernaryWire-Setup-<version>-x64.exe).
;
;   iscc /DAppVersion=0.2.0 /DSourceDir=..\..\target\dist /DOutputDir=..\..\target\dist-pkg packaging\windows\ternarywire.iss
;
; Installs for the current user by default (no administrator rights needed),
; or for all users when chosen in the first dialog. An update installs over
; the previous version (same AppId) and closes a running TernaryWire first.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\target\dist"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\target\dist-pkg"
#endif

[Setup]
AppId={{372CC618-F82C-4402-8842-206E15EB75AB}
AppName=TernaryWire
AppVersion={#AppVersion}
AppVerName=TernaryWire {#AppVersion}
AppPublisher=TernaryWire contributors
AppPublisherURL=https://github.com/ilhamnadhif/ternarywire
AppSupportURL=https://github.com/ilhamnadhif/ternarywire/issues
AppUpdatesURL=https://github.com/ilhamnadhif/ternarywire/releases
DefaultDirName={autopf}\TernaryWire
DefaultGroupName=TernaryWire
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\..\LICENSE-MIT
SetupIconFile=ternarywire.ico
UninstallDisplayIcon={app}\ternarywire-app.exe
UninstallDisplayName=TernaryWire
OutputDir={#OutputDir}
OutputBaseFilename=TernaryWire-Setup-{#AppVersion}-x64
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
ChangesEnvironment=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "addtopath"; Description: "Add the ternarywire command-line tool to PATH"; GroupDescription: "Command line:"

[Files]
Source: "{#SourceDir}\ternarywire-app.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\ternarywire.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "ternarywire.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\TernaryWire"; Filename: "{app}\ternarywire-app.exe"; IconFilename: "{app}\ternarywire.ico"
Name: "{autodesktop}\TernaryWire"; Filename: "{app}\ternarywire-app.exe"; IconFilename: "{app}\ternarywire.ico"; Tasks: desktopicon

[Registry]
; The CLI on PATH (per user, or for all users in an administrative install).
Root: HKA; Subkey: "{code:EnvKey}"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; \
  Check: NeedsAddPath(ExpandConstant('{app}')); Tasks: addtopath

[Run]
Filename: "{app}\ternarywire-app.exe"; Description: "{cm:LaunchProgram,TernaryWire}"; Flags: nowait postinstall skipifsilent

[Code]
function EnvKey(Param: String): String;
begin
  if IsAdminInstallMode then
    Result := 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
  else
    Result := 'Environment';
end;

function NeedsAddPath(Dir: String): Boolean;
var
  Paths: String;
  RootKey: Integer;
begin
  if IsAdminInstallMode then RootKey := HKEY_LOCAL_MACHINE else RootKey := HKEY_CURRENT_USER;
  if not RegQueryStringValue(RootKey, EnvKey(''), 'Path', Paths) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Paths) + ';') = 0;
end;
