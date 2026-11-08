; Mooring for Windows: a per-user install, no administrator rights, unsigned.
; Built by .github/workflows/package-windows.yml from what packaging/windows/stage.py lays out:
;   iscc /DStage=<stage dir> /DVersion=<x.y.z> /O<output dir> packaging\windows\mooring.iss

#ifndef Stage
  #error Pass /DStage=<directory stage.py wrote>
#endif
#ifndef Version
  #define Version "0.1.0"
#endif

[Setup]
AppId={{6A1F5C2E-8C47-4B8E-9E43-3D0B2D7F6A11}
AppName=Mooring
AppVersion={#Version}
AppPublisher=Mooring
AppPublisherURL=https://github.com/majiayu000/mooring
DefaultDirName={localappdata}\Programs\Mooring
DefaultGroupName=Mooring
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=Mooring-Setup-{#Version}
SetupIconFile={#Stage}\Resources\Mooring.ico
UninstallDisplayIcon={app}\Resources\Mooring.ico
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

[Icons]
Name: "{autoprograms}\Mooring"; Filename: "{app}\bin\mooring-app.exe"; IconFilename: "{app}\Resources\Mooring.ico"
Name: "{autodesktop}\Mooring"; Filename: "{app}\bin\mooring-app.exe"; IconFilename: "{app}\Resources\Mooring.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\bin\mooring-app.exe"; Description: "{cm:LaunchProgram,Mooring}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; The host outlives the window; stop this install's own (by path, never another copy's)
; before its files go.
Filename: "powershell.exe"; Parameters: "-NoProfile -NonInteractive -Command ""Get-Process mooring,mooring-app -ErrorAction SilentlyContinue | Where-Object {{ $_.Path -like '{app}\bin\*' } | Stop-Process -Force"""; Flags: runhidden; RunOnceId: "StopMooring"
