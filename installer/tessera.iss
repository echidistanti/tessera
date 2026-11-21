; Inno Setup script for Tessera.
;
; Build it from the repository root, after `cargo build --release`:
;   ISCC.exe /DAppVersion=0.2.0 installer\tessera.iss
; The installer is written to dist\Tessera-Setup-<version>.exe.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

#define AppName "Tessera"
#define AppExe "tessera.exe"
#define AppUrl "https://github.com/echidistanti/tessera"
#define MainWindowClass "Tessera.Main"

[Setup]
; Never change the AppId: it is how Windows recognises an upgrade of the same program.
AppId={{6B0F3C1E-5D7A-4E2B-9A41-7C2E8F5D1B63}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=echidistanti
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; Tessera is a per-user tray program, so no administrator rights are needed. The dialog lets
; someone who wants an all-users install choose it.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\LICENSE
SetupIconFile=..\assets\tessera.ico
UninstallDisplayIcon={app}\{#AppExe}
OutputDir=..\dist
OutputBaseFilename=Tessera-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
MinVersion=10.0

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"

[Run]
; Tessera enables "Start with Windows" itself on its first launch.
Filename: "{app}\{#AppExe}"; Description: "Start {#AppName} now"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; The log file lives outside the installation folder.
Type: filesandordirs; Name: "{localappdata}\Tessera"

[Code]
// Asks a running Tessera to quit (WM_CLOSE), so its files can be replaced or removed.
procedure StopTessera;
var
  Window: HWND;
  Waited: Integer;
begin
  Window := FindWindowByClassName('{#MainWindowClass}');
  if Window = 0 then
    Exit;
  PostMessage(Window, 16, 0, 0); // 16 = WM_CLOSE
  for Waited := 1 to 50 do
  begin
    if FindWindowByClassName('{#MainWindowClass}') = 0 then
      Break;
    Sleep(100);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopTessera;
  Result := '';
end;

function InitializeUninstall: Boolean;
begin
  StopTessera;
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    // Remove what the program itself wrote: the autostart entry and its settings.
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Tessera');
    RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER, 'Software\Tessera');
  end;
end;
