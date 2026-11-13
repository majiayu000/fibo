; Fibo (formerly Mooring) for Windows: a per-user install, no administrator rights, unsigned.
; Built by .github/workflows/package-windows.yml from what packaging/windows/stage.py lays out:
;   iscc /DStage=<stage dir> /DVersion=<x.y.z> /O<output dir> packaging\windows\fibo.iss

#ifndef Stage
  #error Pass /DStage=<directory stage.py wrote>
#endif
#ifndef Version
  #define Version "0.1.0"
#endif

[Setup]
AppId={{6A1F5C2E-8C47-4B8E-9E43-3D0B2D7F6A11}
AppName=Fibo
AppVersion={#Version}
AppPublisher=Fibo
AppPublisherURL=https://fibo.page
; AppId is unchanged, so an existing Mooring install is upgraded in its own folder
; (UsePreviousAppDir); only new installs land in Programs\Fibo.
DefaultDirName={localappdata}\Programs\Fibo
DefaultGroupName=Fibo
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=Fibo-Setup-{#Version}
SetupIconFile={#Stage}\Resources\Fibo.ico
UninstallDisplayIcon={app}\Resources\Fibo.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; The window and its host must be closed to replace them.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Stage}\bin\*"; DestDir: "{app}\bin"; Flags: ignoreversion recursesubdirs
Source: "{#Stage}\Resources\*"; DestDir: "{app}\Resources"; Flags: ignoreversion recursesubdirs

[InstallDelete]
; Shortcuts a Mooring install made; the upgraded install makes Fibo ones instead.
Type: files; Name: "{autoprograms}\Mooring.lnk"
Type: files; Name: "{autodesktop}\Mooring.lnk"

[Icons]
Name: "{autoprograms}\Fibo"; Filename: "{app}\bin\fibo-app.exe"; IconFilename: "{app}\Resources\Fibo.ico"
Name: "{autodesktop}\Fibo"; Filename: "{app}\bin\fibo-app.exe"; IconFilename: "{app}\Resources\Fibo.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\bin\fibo-app.exe"; Description: "{cm:LaunchProgram,Fibo}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; The host outlives the window; stop this install's own (by path, never another copy's)
; before its files go.
Filename: "powershell.exe"; Parameters: "-NoProfile -NonInteractive -Command ""Get-Process fibo,mooring,fibo-app -ErrorAction SilentlyContinue | Where-Object {{ $_.Path -like '{app}\bin\*' } | Stop-Process -Force"""; Flags: runhidden; RunOnceId: "StopMooring"
